/*
 * Copyright 2026 Federico D'Ambrosio
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use crate::limits::{MAX_PANEL_SERIES, MAX_QUERY_BYTES, MAX_QUERY_MESSAGES, MAX_SERIES_POINTS};
use anyhow::{Result, anyhow, ensure};
use reqwest::Client;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

mod tls;

pub(crate) use tls::TlsFiles;
#[cfg(test)]
pub(crate) use tls::tests as tls_tests;

/// Range query results that the client keeps to use again, such as when the user pans back to a window.
const CACHE_CAPACITY: usize = 64;
/// Largest response body read from Prometheus.
const MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
/// Longest excerpt of a response body included in an error message.
const ERROR_EXCERPT_CHARS: usize = 512;
/// Largest size of the note after a cut excerpt, such as `… (123 bytes)`.
const EXCERPT_NOTE_BYTES: usize = 32;

/// The result that the first caller of a query sends to the callers that wait.
type WaiterResult = Result<QueryResult, String>;
type QueryWaiter = tokio::sync::oneshot::Sender<WaiterResult>;
type InflightQueries = Arc<Mutex<HashMap<String, Vec<QueryWaiter>>>>;

/// Waits for the result of a query that another caller sends.
async fn wait_for_inflight(
    rx: tokio::sync::oneshot::Receiver<WaiterResult>,
) -> Result<QueryResult> {
    match rx.await {
        Ok(Ok(res)) => {
            debug_assert!(
                within_limits(&res),
                "the leader sends a result inside the series and point limits"
            );
            Ok(res)
        }
        Ok(Err(s)) => {
            debug_assert!(!s.is_empty(), "the leader sends an error message with text");
            Err(anyhow!(s))
        }
        Err(_) => Err(anyhow!("inflight request cancelled")),
    }
}

/// Returns true when `result` is inside the series, point and message limits.
fn within_limits(result: &QueryResult) -> bool {
    let short = |series: &Series| series.values.len() <= MAX_SERIES_POINTS;
    result.series.len() <= MAX_PANEL_SERIES && result.series.iter().all(short)
}

/// A range query's identity: expression, start, end, and step.
type CacheKey = (String, i64, i64, Duration);

/// Recent range query results. Past `CACHE_CAPACITY`, the cache evicts the oldest.
#[derive(Debug, Default)]
struct QueryCache {
    entries: HashMap<CacheKey, QueryResult>,
    order: VecDeque<CacheKey>,
}

impl QueryCache {
    fn get(&self, key: &CacheKey) -> Option<QueryResult> {
        self.entries.get(key).cloned()
    }

    fn insert(&mut self, key: CacheKey, result: QueryResult) {
        debug_assert!(
            self.entries.len() == self.order.len(),
            "the eviction order holds each cached key one time"
        );
        if self.entries.insert(key.clone(), result).is_none() {
            self.order.push_back(key);
        }
        let excess = self.order.len().saturating_sub(CACHE_CAPACITY);
        for oldest in self.order.drain(..excess) {
            self.entries.remove(&oldest);
        }
        debug_assert!(
            self.entries.len() <= CACHE_CAPACITY,
            "the cache holds CACHE_CAPACITY windows or fewer"
        );
        debug_assert!(
            self.entries.len() == self.order.len(),
            "the eviction order holds each cached key one time"
        );
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Locks `mutex`, and recovers the data if a panic poisoned it. The guarded maps
/// stay consistent across a panic because every update is a single insert or
/// remove.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The first caller's claim on an in-flight range query, which identical
/// concurrent queries wait on.
///
/// When the guard drops, it removes the claim. So if the caller drops the future
/// of a request mid-flight, later identical requests do not wait forever. Their
/// senders drop, and they fail with "inflight request cancelled".
struct InflightGuard {
    inflight: InflightQueries,
    key: String,
}

impl InflightGuard {
    fn take_waiters(&self) -> Vec<QueryWaiter> {
        lock(&self.inflight).remove(&self.key).unwrap_or_default()
    }

    /// Sends the leader's result to every caller that waits, and releases the claim.
    fn publish(self, result: &Result<QueryResult>) {
        debug_assert!(
            result.as_ref().ok().is_none_or(within_limits),
            "the leader publishes a result inside the limits"
        );
        debug_assert!(!self.key.is_empty(), "the claim has a query key");
        for waiter in self.take_waiters() {
            // The send fails only when the caller that waits has stopped and dropped
            // its receiver. No one reads the result then, so the failure does not matter.
            let _delivered: Result<(), WaiterResult> = waiter.send(match result {
                Ok(result) => Ok(result.clone()),
                Err(error) => Err(error.to_string()),
            });
        }
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        // The waiters go out of scope here, so each caller sees a closed channel.
        let _waiters = self.take_waiters();
    }
}

/// A response body over the size limit. The client does not retry it, since
/// the same query returns the same oversized result.
#[derive(Debug)]
struct ResponseTooLarge(usize);

impl std::fmt::Display for ResponseTooLarge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "response exceeds the {} byte limit", self.0)
    }
}

impl std::error::Error for ResponseTooLarge {}

/// The client could not reach Prometheus, or the connection failed mid-response.
/// This is different from an error answer from Prometheus.
#[derive(Debug)]
struct TransportError {
    message: String,
    tls: Option<tls::TlsFailure>,
}

impl TransportError {
    fn new(context: &str, error: &reqwest::Error) -> Self {
        let tls = tls::TlsFailure::find(error);
        let chain = tls::error_chain(error);
        let message = match &tls {
            Some(tls) => format!("{context}: {} ({}): {chain}", tls.cause, tls.hint),
            None => format!("{context}: {chain}"),
        };
        debug_assert!(!context.is_empty(), "the transport error has a context");
        debug_assert!(
            tls.as_ref().is_none_or(|tls| message.contains(&tls.cause)),
            "the message names the TLS cause"
        );
        debug_assert!(
            message.starts_with(context),
            "the message starts with its context"
        );
        Self { message, tls }
    }
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TransportError {}

/// Whether `error` means that the client could not reach Prometheus.
pub(crate) fn is_transport_error(error: &anyhow::Error) -> bool {
    error.is::<TransportError>()
}

/// A short cause, such as `TLS: unknown issuer`, when `error` is a failed TLS
/// handshake, or when the server rejects the client certificate.
pub(crate) fn tls_failure_cause(error: &anyhow::Error) -> Option<&str> {
    let cause = error
        .downcast_ref::<TransportError>()
        .and_then(|error| error.tls.as_ref())
        .map(|tls| tls.cause.as_str());
    debug_assert!(
        cause.is_none() || is_transport_error(error),
        "only a transport error has a TLS cause"
    );
    debug_assert!(
        cause.is_none_or(|cause| !cause.is_empty()),
        "a TLS cause has text"
    );
    cause
}

/// Prometheus answered with an error status.
#[derive(Debug)]
struct StatusError {
    status: reqwest::StatusCode,
    message: String,
}

impl std::fmt::Display for StatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "prometheus {}: {}", self.status, self.message)
    }
}

impl std::error::Error for StatusError {}

impl StatusError {
    /// Uses the `errorType` and `error` of a Prometheus error body. For proxies
    /// and other servers, it uses an excerpt of the body.
    fn new(status: reqwest::StatusCode, body: &str) -> Self {
        #[derive(Deserialize)]
        struct ErrorBody {
            #[serde(rename = "errorType")]
            error_type: Option<String>,
            error: Option<String>,
        }
        let message = match serde_json::from_str::<ErrorBody>(body) {
            Ok(ErrorBody {
                error_type,
                error: Some(error),
            }) => match error_type {
                Some(error_type) => format!("{error_type}: {error}"),
                None => error,
            },
            _ => excerpt(body),
        };
        debug_assert!(!status.is_success(), "a status error has an error status");
        debug_assert!(
            message.len() <= body.len() + EXCERPT_NOTE_BYTES,
            "the message is not longer than the body and the excerpt note"
        );
        Self { status, message }
    }
}

/// Whether another try can succeed: the connection failed, or Prometheus
/// had too much load or was not available. A rejected query fails the same way again,
/// as does a TLS handshake, until the certificates change.
fn is_retryable(error: &anyhow::Error) -> bool {
    let retryable = match error.downcast_ref::<TransportError>() {
        Some(error) => error.tls.is_none(),
        None => error.downcast_ref::<StatusError>().is_some_and(|error| {
            error.status.is_server_error() || error.status == reqwest::StatusCode::TOO_MANY_REQUESTS
        }),
    };
    debug_assert!(
        !retryable || !error.is::<ResponseTooLarge>(),
        "a response over the size limit is not retried"
    );
    debug_assert!(
        !retryable || tls_failure_cause(error).is_none(),
        "a TLS failure is not retried"
    );
    retryable
}

/// A query's series, with the warnings Prometheus returned alongside them.
#[derive(Debug, Clone, Default)]
pub(crate) struct QueryResult {
    pub(crate) series: Vec<Series>,
    /// Response `warnings`, such as partial results.
    pub(crate) warnings: Vec<String>,
    /// Response `infos`, which Prometheus 3 returns for queries that likely
    /// do not mean what they say, such as `rate` over a non-counter name.
    pub(crate) infos: Vec<String>,
}

/// The start of a response body, for error messages.
fn excerpt(text: &str) -> String {
    let shown = match text.char_indices().nth(ERROR_EXCERPT_CHARS) {
        Some((end, _)) => format!(
            "{}… ({} bytes)",
            text.get(..end).unwrap_or(text),
            text.len()
        ),
        None => text.to_string(),
    };
    debug_assert!(
        shown.len() <= text.len() + EXCERPT_NOTE_BYTES,
        "the excerpt is not longer than the text and the excerpt note"
    );
    debug_assert!(
        shown.chars().count() <= ERROR_EXCERPT_CHARS + EXCERPT_NOTE_BYTES,
        "the excerpt has ERROR_EXCERPT_CHARS characters and a short note or fewer"
    );
    shown
}

/// A simple Prometheus HTTP client.
#[derive(Debug, Clone)]
pub(crate) struct PromClient {
    /// Base URL of the Prometheus server.
    pub(crate) base: String,
    /// HTTP client.
    client: reqwest::Client,
    /// Recent range query results.
    cache: Arc<Mutex<QueryCache>>,
    /// In-flight requests: key -> list of waiters
    inflight: InflightQueries,
    /// Largest response body accepted.
    max_response_bytes: usize,
}

impl PromClient {
    pub(crate) fn new(base: String) -> Self {
        let http = Self::client_builder().build().unwrap_or_else(|e| {
            eprintln!(
                "Warning: Failed to configure HTTP client with timeouts: {}",
                e
            );
            eprintln!("         Falling back to default client (requests may hang).");
            Client::new()
        });
        let client = Self::with_client(base, http);
        debug_assert!(
            client.max_response_bytes == MAX_RESPONSE_BYTES,
            "a new client uses the default response size limit"
        );
        client
    }

    /// A client for `base` that uses `tls`. With TLS options set, it fails
    /// and does not fall back to a default client. A default client connects
    /// without the configured certificates.
    pub(crate) fn with_tls(base: String, tls: &TlsFiles) -> Result<Self> {
        if tls.is_empty() {
            return Ok(Self::new(base));
        }
        let http = tls
            .configure(&base, Self::client_builder())?
            .build()
            .map_err(|error| tls.build_error(&error))?;
        Ok(Self::with_client(base, http))
    }

    fn client_builder() -> reqwest::ClientBuilder {
        Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
    }

    fn with_client(base: String, client: Client) -> Self {
        let client = Self {
            base,
            client,
            cache: Arc::new(Mutex::new(QueryCache::default())),
            inflight: Arc::new(Mutex::new(HashMap::new())),
            max_response_bytes: MAX_RESPONSE_BYTES,
        };
        debug_assert!(
            lock(&client.cache).entries.is_empty(),
            "a new client starts with an empty cache"
        );
        debug_assert!(
            lock(&client.inflight).is_empty(),
            "a new client starts with no query in flight"
        );
        client
    }

    pub(crate) fn build_query_range_url(
        &self,
        expr: &str,
        start: i64,
        end: i64,
        step: Duration,
    ) -> String {
        debug_assert!(start <= end, "the query window starts before it ends");
        debug_assert!(!step.is_zero(), "the query step is not zero");
        // Whole seconds keep URLs readable; Prometheus also accepts `ms` steps.
        let step_param = if step.subsec_millis() == 0 {
            format!("{}s", step.as_secs().max(1))
        } else {
            format!("{}ms", step.as_millis())
        };
        format!(
            "{}/api/v1/query_range?query={}&start={}&end={}&step={}",
            self.base.trim_end_matches('/'),
            urlencoding::encode(expr),
            start,
            end,
            step_param
        )
    }

    pub(crate) fn build_query_url(&self, expr: &str, time: i64) -> String {
        let base = self.base.trim_end_matches('/');
        let url = format!(
            "{base}/api/v1/query?query={}&time={time}",
            urlencoding::encode(expr)
        );
        debug_assert!(url.starts_with(base), "the URL starts with the base URL");
        debug_assert!(
            url.len() >= base.len() + expr.len(),
            "the encoded query is not shorter than the query"
        );
        url
    }

    pub(crate) async fn query_range(
        &self,
        expr: &str,
        start: i64,
        end: i64,
        step: Duration,
    ) -> Result<QueryResult> {
        debug_assert!(start <= end, "the query window starts before it ends");
        debug_assert!(!step.is_zero(), "the query step is not zero");
        check_query_len(expr)?;
        let cache_key = (expr.to_string(), start, end, step);
        if let Some(result) = lock(&self.cache).get(&cache_key) {
            return Ok(result);
        }

        let inflight_key = format!("{}|{}|{}|{}", expr, start, end, step.as_millis());
        let claim = match self.claim_inflight(inflight_key) {
            Ok(claim) => claim,
            Err(rx) => return wait_for_inflight(rx).await,
        };

        let url = self.build_query_range_url(expr, start, end, step);
        let final_res = self.request_with_retries(&url).await;
        if let Ok(result) = &final_res {
            lock(&self.cache).insert(cache_key, result.clone());
        }

        debug_assert!(
            final_res.as_ref().ok().is_none_or(within_limits),
            "the range query result is inside the series and point limits"
        );
        claim.publish(&final_res);
        final_res
    }

    /// Claims the query `key`, or returns a receiver for the result of the
    /// caller that claimed it first.
    fn claim_inflight(
        &self,
        key: String,
    ) -> std::result::Result<InflightGuard, tokio::sync::oneshot::Receiver<WaiterResult>> {
        let mut inflight = lock(&self.inflight);
        if let Some(waiters) = inflight.get_mut(&key) {
            let (tx, rx) = tokio::sync::oneshot::channel();
            waiters.push(tx);
            debug_assert!(!waiters.is_empty(), "the claim lists the new waiter");
            Err(rx)
        } else {
            let earlier = inflight.insert(key.clone(), Vec::new());
            debug_assert!(earlier.is_none(), "no other caller holds the claim");
            Ok(InflightGuard {
                inflight: Arc::clone(&self.inflight),
                key,
            })
        }
    }

    /// Sends a range query to `url`, and tries again while the error is retryable.
    async fn request_with_retries(&self, url: &str) -> Result<QueryResult> {
        let max_retries = 3;
        let mut last_err = anyhow!("unknown error");

        for attempt in 0..=max_retries {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
            }

            match self.perform_request(url).await {
                Ok(result) => return Ok(result),
                Err(e) if !is_retryable(&e) => return Err(e),
                Err(e) => last_err = e,
            }
        }
        debug_assert!(
            is_retryable(&last_err),
            "only a retryable error uses all the attempts"
        );
        debug_assert!(
            !last_err.is::<ResponseTooLarge>(),
            "a response over the size limit is not retried"
        );
        Err(last_err)
    }

    async fn perform_request(&self, url: &str) -> Result<QueryResult> {
        let text = self.get_text(url).await?;

        let body: PromResponse<QueryRangeData> = serde_json::from_str(&text)
            .map_err(|e| anyhow!("parsing json: {} (body: {})", e, excerpt(&text)))?;
        debug_assert!(!text.is_empty(), "a JSON body has text");

        ensure!(
            body.status == "success",
            "prometheus error status: {} — body: {}",
            body.status,
            excerpt(&text)
        );

        Ok(limit_result(QueryResult {
            series: body.data.result,
            warnings: body.warnings,
            infos: body.infos,
        }))
    }

    /// Values of `label` on series in `start..=end`, as Grafana's `label_values`
    /// queries. With a `selector`, only the series that match it count.
    pub(crate) async fn label_values(
        &self,
        label: &str,
        selector: Option<&str>,
        start: i64,
        end: i64,
    ) -> Result<Vec<String>> {
        debug_assert!(start <= end, "the time window starts before it ends");
        check_query_len(label)?;
        check_query_len(selector.unwrap_or_default())?;
        let mut url = format!(
            "{}/api/v1/label/{}/values?start={}&end={}",
            self.base.trim_end_matches('/'),
            urlencoding::encode(label),
            start,
            end
        );
        if let Some(selector) = selector {
            url.push_str("&match[]=");
            url.push_str(&urlencoding::encode(selector));
        }
        debug_assert!(
            url.starts_with(self.base.trim_end_matches('/')),
            "the URL starts with the base URL"
        );
        let body: PromResponse<Vec<String>> = self.get_json(&url).await?;
        ensure!(
            body.status == "success",
            "prometheus error status: {}",
            body.status
        );
        Ok(body.data)
    }

    pub(crate) async fn query_instant_result_strings(
        &self,
        expr: &str,
        time: i64,
    ) -> Result<Vec<String>> {
        check_query_len(expr)?;
        let url = self.build_query_url(expr, time);
        let body: PromResponse<QueryInstantData> = self.get_json(&url).await?;
        ensure!(
            body.status == "success",
            "prometheus error status: {}",
            body.status
        );
        Ok(body.data.result_strings())
    }

    pub(crate) async fn query_instant_series(&self, expr: &str, time: i64) -> Result<QueryResult> {
        check_query_len(expr)?;
        let url = self.build_query_url(expr, time);
        let body: PromResponse<QueryInstantData> = self.get_json(&url).await?;
        ensure!(
            body.status == "success",
            "prometheus error status: {}",
            body.status
        );
        let result = limit_result(QueryResult {
            series: body.data.into_series(time),
            warnings: body.warnings,
            infos: body.infos,
        });
        debug_assert!(
            result.series.iter().all(|series| series.values.len() == 1),
            "each instant series keeps its one point"
        );
        Ok(result)
    }

    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let text = self.get_text(url).await?;
        serde_json::from_str(&text)
            .map_err(|e| anyhow!("parsing json: {} (body: {})", e, excerpt(&text)))
    }

    /// Reads a response body of at most `max_response_bytes`, so one huge
    /// result cannot exhaust memory.
    async fn get_text(&self, url: &str) -> Result<String> {
        let mut resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| TransportError::new("request failed", &e))?;
        let status = resp.status();
        let limit = self.max_response_bytes;
        debug_assert!(limit > 0, "the response size limit is not zero");
        let too_large = || anyhow::Error::new(ResponseTooLarge(limit));
        if resp
            .content_length()
            .is_some_and(|length| length > crate::num::usize_to_u64(limit))
        {
            return Err(too_large());
        }
        let body = read_body(&mut resp, limit).await?;
        debug_assert!(body.len() <= limit, "the body is inside the size limit");
        let text = String::from_utf8(body)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned());

        if !status.is_success() {
            return Err(StatusError::new(status, &text).into());
        }

        // A lossy conversion adds 2 bytes for each bad byte. Valid UTF-8 adds no bytes.
        debug_assert!(
            text.len() <= limit.saturating_mul(3),
            "the text is inside the size limit after a lossy conversion"
        );
        Ok(text)
    }
}

/// Reads the body of `resp` to its end. A body over `limit` bytes is an error.
async fn read_body(resp: &mut reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    // A chunk holds one byte or more, so `limit + 1` reads reach the end or pass the limit.
    for _ in 0..=limit {
        let chunk = resp
            .chunk()
            .await
            .map_err(|e| TransportError::new("reading text", &e))?;
        let Some(chunk) = chunk else {
            return Ok(body);
        };
        if body.len() + chunk.len() > limit {
            break;
        }
        body.extend_from_slice(&chunk);
        debug_assert!(body.len() <= limit, "the body is inside the size limit");
    }
    debug_assert!(
        body.len() <= limit,
        "the body stops before it passes the size limit"
    );
    Err(anyhow::Error::new(ResponseTooLarge(limit)))
}

/// Fails when `query` has more than `MAX_QUERY_BYTES` bytes.
fn check_query_len(query: &str) -> Result<()> {
    ensure!(
        query.len() <= MAX_QUERY_BYTES,
        "the query has {} bytes, more than the limit of {MAX_QUERY_BYTES} bytes",
        query.len()
    );
    Ok(())
}

/// Cuts `result` to the series, point and message limits. Each cut adds a
/// warning, so the panel shows it.
fn limit_result(mut result: QueryResult) -> QueryResult {
    let mut cuts = Vec::new();
    let series = result.series.len();
    if series > MAX_PANEL_SERIES {
        result.series.truncate(MAX_PANEL_SERIES);
        cuts.push(format!(
            "{series} series: the panel keeps the first {MAX_PANEL_SERIES} (the series limit)"
        ));
    }
    let mut long_series = 0;
    for series in &mut result.series {
        let excess = series.values.len().saturating_sub(MAX_SERIES_POINTS);
        if excess > 0 {
            series.values.drain(..excess);
            long_series += 1;
        }
    }
    if long_series > 0 {
        cuts.push(format!(
            "{long_series} series have more than {MAX_SERIES_POINTS} points: the panel keeps \
             the newest (the point limit)"
        ));
    }
    cuts.extend(cut_messages(&mut result.warnings, "warnings"));
    cuts.extend(cut_messages(&mut result.infos, "infos"));
    debug_assert!(
        cuts.len() <= 4,
        "each of the four limits adds one warning or none"
    );
    result.warnings.extend(cuts);
    debug_assert!(
        within_limits(&result),
        "the result is inside the series and point limits"
    );
    debug_assert!(
        result.warnings.len() <= MAX_QUERY_MESSAGES + 4 && result.infos.len() <= MAX_QUERY_MESSAGES,
        "the messages are inside the message limit"
    );
    result
}

/// Cuts `messages` to `MAX_QUERY_MESSAGES`, and gives a warning when it cuts.
fn cut_messages(messages: &mut Vec<String>, kind: &str) -> Option<String> {
    debug_assert!(!kind.is_empty(), "the message kind has a name");
    let excess = messages.len().saturating_sub(MAX_QUERY_MESSAGES);
    messages.truncate(MAX_QUERY_MESSAGES);
    debug_assert!(
        messages.len() <= MAX_QUERY_MESSAGES,
        "the messages are inside the message limit"
    );
    (excess > 0).then(|| {
        format!("{excess} more Prometheus {kind} are not shown (the limit is {MAX_QUERY_MESSAGES})")
    })
}

#[derive(Debug, Deserialize, Clone)]
struct PromResponse<T> {
    status: String,
    data: T,
    #[serde(default)]
    warnings: Vec<String>,
    #[serde(default)]
    infos: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct QueryRangeData {
    pub(crate) result: Vec<Series>,
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct Series {
    pub(crate) metric: std::collections::HashMap<String, String>,
    pub(crate) values: Vec<(f64, String)>, // (ts, value)
}

#[derive(Debug, Deserialize, Clone)]
struct QueryInstantData {
    #[serde(rename = "resultType")]
    result_type: String,
    result: serde_json::Value,
}

impl QueryInstantData {
    fn result_strings(self) -> Vec<String> {
        let strings = match self.result_type.as_str() {
            "vector" => vector_result_strings(&self.result),
            "scalar" | "string" => scalar_result_string(&self.result).into_iter().collect(),
            _ => Vec::new(),
        };
        debug_assert!(
            strings.len() <= sample_count(&self.result),
            "each sample gives one string or none"
        );
        debug_assert!(
            strings.is_empty() || self.result.is_array(),
            "only an array result gives strings"
        );
        strings
    }

    fn into_series(self, time: i64) -> Vec<Series> {
        let series = match self.result_type.as_str() {
            "vector" => vector_result_series(&self.result, time),
            "scalar" => scalar_result_series(&self.result, time)
                .into_iter()
                .collect(),
            _ => Vec::new(),
        };
        debug_assert!(
            series.len() <= sample_count(&self.result),
            "each sample gives one series or none"
        );
        debug_assert!(
            series.iter().all(|series| series.values.len() == 1),
            "each instant series has one point"
        );
        series
    }
}

/// The number of samples in an instant query result: the length of a vector,
/// or 1 for a scalar or a string.
fn sample_count(result: &serde_json::Value) -> usize {
    result.as_array().map_or(0, Vec::len).max(1)
}

fn vector_result_series(result: &serde_json::Value, time: i64) -> Vec<Series> {
    let series: Vec<Series> = result
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|sample| {
            let metric = sample.get("metric")?.as_object()?;
            let value = sample
                .get("value")
                .and_then(|value| value.as_array())
                .and_then(|value| value.get(1))
                .and_then(|value| value.as_str())?;

            Some(Series {
                metric: metric
                    .iter()
                    .filter_map(|(label, value)| {
                        value
                            .as_str()
                            .map(|value| (label.clone(), value.to_string()))
                    })
                    .collect(),
                values: vec![(crate::num::i64_to_f64(time), value.to_string())],
            })
        })
        .collect();
    debug_assert!(
        series.len() <= sample_count(result),
        "each sample gives one series or none"
    );
    debug_assert!(
        series.iter().all(|series| series.values.len() == 1),
        "each instant series has one point"
    );
    series
}

fn scalar_result_series(result: &serde_json::Value, time: i64) -> Option<Series> {
    let value = result
        .as_array()
        .and_then(|value| value.get(1))
        .and_then(|value| value.as_str())?;
    debug_assert!(result.is_array(), "a scalar result is an array");

    let series = Series {
        metric: HashMap::new(),
        values: vec![(crate::num::i64_to_f64(time), value.to_string())],
    };
    debug_assert!(series.values.len() == 1, "a scalar series has one point");
    Some(series)
}

fn vector_result_strings(result: &serde_json::Value) -> Vec<String> {
    let strings: Vec<String> = result
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|sample| {
            let metric = sample.get("metric")?.as_object()?;
            let value = sample
                .get("value")
                .and_then(|value| value.as_array())
                .and_then(|value| value.get(1))
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let mut labels: Vec<_> = metric
                .iter()
                .filter_map(|(label, value)| value.as_str().map(|value| (label, value)))
                .collect();
            labels.sort_by(|a, b| a.0.cmp(b.0));
            let labels = labels
                .into_iter()
                .map(|(label, value)| format!("{}=\"{}\"", label, value))
                .collect::<Vec<_>>()
                .join(", ");

            if labels.is_empty() {
                Some(value.to_string())
            } else {
                Some(format!("{{{}}} {}", labels, value))
            }
        })
        .collect();
    debug_assert!(
        strings.len() <= sample_count(result),
        "each sample gives one string or none"
    );
    debug_assert!(
        strings.is_empty() || result.is_array(),
        "only an array result gives strings"
    );
    strings
}

fn scalar_result_string(result: &serde_json::Value) -> Option<String> {
    let value = result
        .as_array()
        .and_then(|value| value.get(1))
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    debug_assert!(
        value.is_none() || result.is_array(),
        "only an array result gives a value"
    );
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const EMPTY_MATRIX: &str = r#"{"status":"success","data":{"resultType":"matrix","result":[]}}"#;

    /// A Prometheus stand-in. It accepts the first `stalled` connections and
    /// never answers them; later ones receive `response` (a full HTTP response).
    async fn server(stalled: usize, response: String) -> (String, Arc<tokio::sync::Notify>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let reached = Arc::new(tokio::sync::Notify::new());
        let notify = Arc::clone(&reached);
        tokio::spawn(async move {
            let mut held = Vec::new();
            let mut accepted = 0;
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                accepted += 1;
                let mut buffer = [0_u8; 4096];
                let _ = socket.read(&mut buffer).await;
                notify.notify_one();
                if accepted <= stalled {
                    held.push(socket);
                } else {
                    let _ = socket.write_all(response.as_bytes()).await;
                }
            }
        });
        (format!("http://{address}"), reached)
    }

    fn ok_response(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[tokio::test]
    async fn a_dropped_query_does_not_strand_identical_queries() {
        let (url, _) = server(1, ok_response(EMPTY_MATRIX)).await;
        let client = PromClient::new(url);
        let step = Duration::from_secs(15);

        let abandoned = tokio::time::timeout(
            Duration::from_millis(200),
            client.query_range("up", 0, 60, step),
        )
        .await;
        assert!(abandoned.is_err(), "the stalled request should time out");

        let retried = tokio::time::timeout(
            Duration::from_secs(2),
            client.query_range("up", 0, 60, step),
        )
        .await
        .expect("a later identical query must not wait on the dropped one");
        assert!(retried.unwrap().series.is_empty());
        assert!(lock(&client.inflight).is_empty());
    }

    #[tokio::test]
    async fn waiters_fail_promptly_when_the_leading_query_is_cancelled() {
        let (url, reached) = server(1, ok_response(EMPTY_MATRIX)).await;
        let client = PromClient::new(url);
        let step = Duration::from_secs(15);
        let leader = tokio::spawn({
            let client = client.clone();
            async move { client.query_range("up", 0, 60, step).await }
        });
        reached.notified().await;
        let follower = tokio::spawn({
            let client = client.clone();
            async move { client.query_range("up", 0, 60, step).await }
        });
        while lock(&client.inflight)
            .values()
            .all(|waiters| waiters.is_empty())
        {
            tokio::task::yield_now().await;
        }

        leader.abort();

        let error = tokio::time::timeout(Duration::from_secs(2), follower)
            .await
            .expect("the follower must not hang")
            .unwrap()
            .unwrap_err();
        assert!(error.to_string().contains("cancelled"), "{error}");
        assert!(lock(&client.inflight).is_empty());
    }

    /// Answers every request with `response`, and counts the requests.
    async fn counting_server(response: String) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = Arc::clone(&requests);
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buffer = [0_u8; 4096];
                let _ = socket.read(&mut buffer).await;
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        (format!("http://{address}"), requests)
    }

    fn status_response(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[tokio::test]
    async fn query_results_carry_warnings_and_infos() {
        let range = r#"{"status":"success","data":{"resultType":"matrix","result":[]},
            "warnings":["partial response"],"infos":["metric might not be a counter"]}"#;
        let (url, _) = counting_server(ok_response(range)).await;
        let client = PromClient::new(url);
        let result = client
            .query_range("up", 0, 60, Duration::from_secs(15))
            .await
            .unwrap();
        assert_eq!(result.warnings, ["partial response"]);
        assert_eq!(result.infos, ["metric might not be a counter"]);

        let instant = r#"{"status":"success","data":{"resultType":"vector","result":[]},
            "warnings":["partial response"]}"#;
        let (url, _) = counting_server(ok_response(instant)).await;
        let result = PromClient::new(url)
            .query_instant_series("up", 60)
            .await
            .unwrap();
        assert_eq!(result.warnings, ["partial response"]);
        assert!(result.infos.is_empty());
    }

    #[tokio::test]
    async fn rejected_queries_are_not_retried() {
        let body = r#"{"status":"error","errorType":"bad_data","error":"parse error at char 4"}"#;
        let (url, requests) = counting_server(status_response("400 Bad Request", body)).await;

        let error = PromClient::new(url)
            .query_range("up{", 0, 60, Duration::from_secs(15))
            .await
            .unwrap_err();

        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            error.to_string(),
            "prometheus 400 Bad Request: bad_data: parse error at char 4"
        );
        assert!(!is_transport_error(&error));
    }

    #[tokio::test]
    async fn unavailable_prometheus_is_retried() {
        let body = r#"{"status":"error","errorType":"unavailable","error":"too many queries"}"#;
        let (url, requests) =
            counting_server(status_response("503 Service Unavailable", body)).await;

        PromClient::new(url)
            .query_range("up", 0, 60, Duration::from_secs(15))
            .await
            .unwrap_err();

        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 4);
    }

    #[test]
    fn non_prometheus_error_bodies_keep_an_excerpt() {
        let error = StatusError::new(reqwest::StatusCode::BAD_GATEWAY, "<html>bad gateway</html>");
        assert_eq!(
            error.to_string(),
            "prometheus 502 Bad Gateway: <html>bad gateway</html>"
        );
    }

    /// A series with `points` points at the times 0, 1, 2 and on.
    fn series_with_points(points: usize) -> Series {
        Series {
            metric: HashMap::new(),
            values: std::iter::successors(Some(0.0), |time| Some(time + 1.0))
                .take(points)
                .map(|time| (time, "1".to_string()))
                .collect(),
        }
    }

    #[test]
    fn results_in_the_limits_stay_the_same() {
        let result = limit_result(QueryResult {
            series: vec![series_with_points(MAX_SERIES_POINTS)],
            warnings: vec!["partial".to_string(); MAX_QUERY_MESSAGES],
            infos: Vec::new(),
        });
        assert_eq!(result.series[0].values.len(), MAX_SERIES_POINTS);
        assert_eq!(result.warnings.len(), MAX_QUERY_MESSAGES);
    }

    #[test]
    fn series_over_the_limit_are_cut_with_a_warning() {
        let result = limit_result(QueryResult {
            series: vec![series_with_points(1); MAX_PANEL_SERIES + 5],
            ..QueryResult::default()
        });
        assert_eq!(result.series.len(), MAX_PANEL_SERIES);
        assert_eq!(
            result.warnings,
            ["2005 series: the panel keeps the first 2000 (the series limit)"]
        );
    }

    #[test]
    fn points_over_the_limit_are_cut_to_the_newest_with_a_warning() {
        let result = limit_result(QueryResult {
            series: vec![
                series_with_points(MAX_SERIES_POINTS + 3),
                series_with_points(2),
            ],
            ..QueryResult::default()
        });
        assert_eq!(result.series[0].values.len(), MAX_SERIES_POINTS);
        assert_eq!(result.series[0].values[0].0, 3.0);
        assert_eq!(result.series[1].values.len(), 2);
        assert_eq!(
            result.warnings,
            ["1 series have more than 22000 points: the panel keeps the newest (the point limit)"]
        );
    }

    #[test]
    fn messages_over_the_limit_are_cut_with_a_warning() {
        let result = limit_result(QueryResult {
            series: Vec::new(),
            warnings: vec!["w".to_string(); MAX_QUERY_MESSAGES + 2],
            infos: vec!["i".to_string(); MAX_QUERY_MESSAGES + 1],
        });
        assert_eq!(result.infos.len(), MAX_QUERY_MESSAGES);
        assert_eq!(result.warnings.len(), MAX_QUERY_MESSAGES + 2);
        assert_eq!(
            result.warnings[MAX_QUERY_MESSAGES..],
            [
                "2 more Prometheus warnings are not shown (the limit is 32)",
                "1 more Prometheus infos are not shown (the limit is 32)",
            ]
        );
    }

    #[tokio::test]
    async fn queries_over_the_length_limit_are_not_sent() {
        let client = PromClient::new("http://127.0.0.1:9".to_string());
        let long = "a".repeat(MAX_QUERY_BYTES + 1);
        let expected = format!("the query has {} bytes, more than the limit of", long.len());
        let step = Duration::from_secs(15);
        let errors = [
            client.query_range(&long, 0, 60, step).await.unwrap_err(),
            client.query_instant_series(&long, 0).await.unwrap_err(),
            client
                .query_instant_result_strings(&long, 0)
                .await
                .unwrap_err(),
            client.label_values(&long, None, 0, 60).await.unwrap_err(),
            client
                .label_values("job", Some(&long), 0, 60)
                .await
                .unwrap_err(),
        ];
        for error in errors {
            assert!(error.to_string().starts_with(&expected), "{error}");
        }
    }

    #[test]
    fn the_query_cache_keeps_only_recent_windows() {
        let mut cache = QueryCache::default();
        let step = Duration::from_secs(15);
        for end in 0..1000 {
            cache.insert(
                ("up".to_string(), end - 60, end, step),
                QueryResult::default(),
            );
        }

        assert_eq!(cache.len(), CACHE_CAPACITY);
        assert!(cache.get(&("up".to_string(), 939, 999, step)).is_some());
        assert!(cache.get(&("up".to_string(), -60, 0, step)).is_none());
    }

    #[tokio::test]
    async fn oversized_responses_are_rejected() {
        let body = format!("{{\"padding\":\"{}\"}}", "x".repeat(4096));
        // With a declared length, and streamed until the connection closes.
        let streamed = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}"
        );
        for response in [ok_response(&body), streamed] {
            let (url, _) = server(0, response).await;
            let mut client = PromClient::new(url);
            client.max_response_bytes = 1024;

            let error = client
                .query_range("up", 0, 60, Duration::from_secs(15))
                .await
                .unwrap_err();

            assert!(error.to_string().contains("byte limit"), "{error}");
        }
    }

    #[test]
    fn error_excerpts_are_truncated_on_character_boundaries() {
        assert_eq!(excerpt("short"), "short");
        let long = "é".repeat(ERROR_EXCERPT_CHARS + 10);
        let shown = excerpt(&long);
        assert!(shown.starts_with(&"é".repeat(ERROR_EXCERPT_CHARS)));
        assert!(
            shown.ends_with(&format!("… ({} bytes)", long.len())),
            "{shown}"
        );
    }

    #[test]
    fn test_build_query_range_url() {
        let client = PromClient::new("http://localhost:9090".to_string());
        let expr = "up{job=\"node\"}";
        let start = 1600000000;
        let end = 1600003600;
        let step = Duration::from_secs(60);

        let url = client.build_query_range_url(expr, start, end, step);
        assert_eq!(
            url,
            "http://localhost:9090/api/v1/query_range?query=up%7Bjob%3D%22node%22%7D&start=1600000000&end=1600003600&step=60s"
        );
    }

    #[test]
    fn query_range_url_keeps_sub_second_steps() {
        let client = PromClient::new("http://localhost:9090".to_string());
        let url = client.build_query_range_url("up", 0, 60, Duration::from_millis(1500));
        assert!(url.ends_with("&step=1500ms"), "{url}");
    }

    #[test]
    fn test_build_query_url_preserves_path_prefix() {
        let client = PromClient::new("http://localhost:9090/prometheus/".to_string());
        let url = client.build_query_url("up{job=\"node\"}", 1600003600);

        assert_eq!(
            url,
            "http://localhost:9090/prometheus/api/v1/query?query=up%7Bjob%3D%22node%22%7D&time=1600003600"
        );
    }

    #[test]
    fn test_deserialize_query_range_response() {
        let json = r#"
        {
            "status": "success",
            "data": {
                "resultType": "matrix",
                "result": [
                    {
                        "metric": {
                            "__name__": "up",
                            "job": "prometheus"
                        },
                        "values": [
                            [1435781451.781, "1"],
                            [1435781466.781, "1"]
                        ]
                    }
                ]
            }
        }
        "#;

        let resp: PromResponse<QueryRangeData> = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "success");
        assert_eq!(resp.data.result.len(), 1);
        assert_eq!(resp.data.result[0].metric.get("job").unwrap(), "prometheus");
        assert_eq!(resp.data.result[0].values.len(), 2);
    }

    #[test]
    fn test_query_instant_vector_result_strings() {
        let json = r#"
        {
            "resultType": "vector",
            "result": [
                {
                    "metric": { "instance": "node-1", "job": "node" },
                    "value": [1435781451.781, "1"]
                }
            ]
        }
        "#;

        let data: QueryInstantData = serde_json::from_str(json).unwrap();

        assert_eq!(
            data.result_strings(),
            vec![r#"{instance="node-1", job="node"} 1"#]
        );
    }

    #[test]
    fn test_query_instant_vector_converts_to_series() {
        let json = r#"
        {
            "resultType": "vector",
            "result": [
                {
                    "metric": { "instance": "node-1", "job": "node" },
                    "value": [1435781451.781, "1"]
                },
                {
                    "metric": { "instance": "node-2", "job": "node" },
                    "value": [1435781451.781, "2.5"]
                }
            ]
        }
        "#;

        let data: QueryInstantData = serde_json::from_str(json).unwrap();
        let series = data.into_series(1_435_781_451);

        assert_eq!(series.len(), 2);
        assert_eq!(series[0].metric.get("instance").unwrap(), "node-1");
        assert_eq!(series[0].values, vec![(1_435_781_451.0, "1".to_string())]);
        assert_eq!(series[1].metric.get("instance").unwrap(), "node-2");
        assert_eq!(series[1].values, vec![(1_435_781_451.0, "2.5".to_string())]);
    }

    #[test]
    fn test_query_instant_scalar_converts_to_unlabeled_series() {
        let json = r#"
        {
            "resultType": "scalar",
            "result": [1435781451.781, "42"]
        }
        "#;

        let data: QueryInstantData = serde_json::from_str(json).unwrap();
        let series = data.into_series(1_435_781_451);

        assert_eq!(series.len(), 1);
        assert!(series[0].metric.is_empty());
        assert_eq!(series[0].values, vec![(1_435_781_451.0, "42".to_string())]);
    }
}
