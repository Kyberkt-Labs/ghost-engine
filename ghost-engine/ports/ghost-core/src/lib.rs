mod resource_reader;

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, Once};
use std::time::{Duration, Instant};

use servo::{
    Opts, Preferences, RenderingContext, Servo, ServoBuilder,
    SoftwareRenderingContext, UserContentManager, WebView, WebViewBuilder, WebViewDelegate,
    WebResourceLoad,
};
use url::Url;

// Re-export LoadStatus so callers don't need a direct servo dependency.
pub use servo::LoadStatus;
// Re-export JS evaluation types used by ghost-interceptor.
pub use servo::{JSValue, JavaScriptEvaluationError};

// ── Global one-shot init ────────────────────────────────────────────────────

static RUNTIME_INIT: Once = Once::new();

fn ensure_runtime_initialized(config: &GhostEngineConfig) {
    RUNTIME_INIT.call_once(|| {
        servoshell::init_crypto();
        servoshell::init_tracing(config.tracing_filter.as_deref());
        resource_reader::init();
    });
}

// ── Configuration ───────────────────────────────────────────────────────────

/// Resource types that can be blocked during page loads to save bandwidth
/// and memory. Agents rarely need images, fonts, or media to understand
/// page content — layout extraction captures text and structure.
#[derive(Debug, Clone, Default)]
pub struct ResourceBudget {
    /// Block image downloads (jpg, png, gif, webp, svg, ico, avif).
    /// Default: `false`.
    pub skip_images: bool,
    /// Block web font downloads (woff, woff2, ttf, otf, eot).
    /// Default: `false`.
    pub skip_fonts: bool,
    /// Block media downloads (mp4, webm, mp3, ogg, wav, flac, m3u8).
    /// Default: `false`.
    pub skip_media: bool,
    /// Block stylesheet downloads (CSS files). Use with caution — disabling
    /// styles may affect layout extraction accuracy.
    /// Default: `false`.
    pub skip_stylesheets: bool,
    /// Maximum allowed size (in bytes) for any single sub-resource response.
    /// Resources whose `Content-Length` exceeds this limit are cancelled.
    /// `0` means no limit. Default: `0`.
    pub max_resource_bytes: u64,
}

/// Configuration for a [`GhostEngine`] instance.
#[derive(Debug, Clone)]
pub struct GhostEngineConfig {
    /// Viewport width in device pixels (default: 1920).
    pub viewport_width: u32,
    /// Viewport height in device pixels (default: 1080).
    pub viewport_height: u32,
    /// Optional tracing/log filter directive (e.g. `"servo=debug"`).
    pub tracing_filter: Option<String>,
    /// Hard limit on how long [`GhostEngine::load_and_wait`] will spin the
    /// event loop waiting for page load (default: 30 s).
    pub load_timeout: Duration,
    /// After page load completes, keep spinning the event loop for this
    /// duration to let async JS (setTimeout, fetch callbacks, Promises)
    /// finish executing. The loop exits early if no waker activity is
    /// observed for [`Self::quiet_period`]. Set to zero to skip settling.
    /// Default: 2 s.
    pub settle_timeout: Duration,
    /// If the event-loop waker is not signalled for this long during the
    /// settle phase, assume JS has gone idle and stop early.
    /// Default: 500 ms.
    pub quiet_period: Duration,
    /// Custom User-Agent string. When `None`, Servo's default UA is used.
    pub user_agent: Option<String>,

    // ── Performance & memory (TSK-5.5–5.8) ──────────────────────────────

    /// Resource budget controls for blocking heavy sub-resources.
    /// Default: everything allowed (no blocking).
    pub resource_budget: ResourceBudget,
    /// TCP connection timeout for HTTP requests (default: 10 s).
    /// Maps to Servo's `network_connection_timeout` preference.
    pub connection_timeout: Duration,
    /// Whether to enable Servo's built-in HTTP disk cache (default: `true`).
    /// Disabling the cache ensures fresh content but increases network load.
    pub http_cache_enabled: bool,
    /// Weight-based capacity for the HTTP cache (default: `5000`).
    /// Higher values allow more cached responses. Only relevant when
    /// [`Self::http_cache_enabled`] is `true`.
    pub http_cache_size: u64,
}

impl Default for GhostEngineConfig {
    fn default() -> Self {
        Self {
            viewport_width: 1920,
            viewport_height: 1080,
            tracing_filter: None,
            load_timeout: Duration::from_secs(30),
            settle_timeout: Duration::from_secs(2),
            quiet_period: Duration::from_millis(500),
            user_agent: None,
            resource_budget: ResourceBudget::default(),
            connection_timeout: Duration::from_secs(10),
            http_cache_enabled: true,
            http_cache_size: 5000,
        }
    }
}

// ── Event-loop waker (headless) ─────────────────────────────────────────────

#[derive(Clone)]
struct HeadlessWaker {
    inner: Arc<HeadlessWakerInner>,
}

struct HeadlessWakerInner {
    flag: Mutex<bool>,
    condvar: Condvar,
}

impl HeadlessWaker {
    fn new() -> Self {
        Self {
            inner: Arc::new(HeadlessWakerInner {
                flag: Mutex::new(false),
                condvar: Condvar::new(),
            }),
        }
    }

    /// Block the current thread until woken or `timeout` elapses.
    /// Returns `true` if woken by a signal, `false` on timeout.
    fn sleep(&self, timeout: Duration) -> bool {
        let guard = self.inner.flag.lock().unwrap();
        if *guard {
            return true;
        }
        let (guard, result) = self.inner.condvar.wait_timeout(guard, timeout).unwrap();
        // If the flag was set while we waited, we were woken by a signal.
        *guard || !result.timed_out()
    }

    /// Reset the flag after processing.
    fn clear(&self) {
        *self.inner.flag.lock().unwrap() = false;
    }
}

impl servo::EventLoopWaker for HeadlessWaker {
    fn wake(&self) {
        *self.inner.flag.lock().unwrap() = true;
        self.inner.condvar.notify_all();
    }

    fn clone_box(&self) -> Box<dyn servo::EventLoopWaker> {
        Box::new(self.clone())
    }
}

// ── Page-load lifecycle ─────────────────────────────────────────────────────

/// Tracks a page load through Servo's lifecycle phases.
///
/// Servo emits three [`LoadStatus`] transitions to the embedder:
///
/// | Phase | `LoadStatus` | Equivalent |
/// |-------|-------------|------------|
/// | Navigation begun | `Started` | `readyState = "loading"` |
/// | `<head>` parsed, `<body>` inserted | `HeadParsed` | (no web equivalent) |
/// | All sub-resources loaded | `Complete` | `readyState = "complete"` / `window.load` |
///
/// **Note:** Servo does not expose a separate embedder signal for
/// `DOMContentLoaded` (`readyState = "interactive"`). The closest
/// earlier milestone is `HeadParsed`.
#[derive(Debug, Clone)]
pub struct PageLoadProgress {
    /// Timestamp when the load was initiated (before any `LoadStatus` fires).
    pub initiated_at: Instant,
    /// Timestamp when `LoadStatus::Started` was observed.
    pub started_at: Option<Instant>,
    /// Timestamp when `LoadStatus::HeadParsed` was observed.
    pub head_parsed_at: Option<Instant>,
    /// Timestamp when `LoadStatus::Complete` was observed.
    pub complete_at: Option<Instant>,
    /// The most recent [`LoadStatus`] delivered by Servo.
    pub current: Option<LoadStatus>,
}

impl PageLoadProgress {
    fn new() -> Self {
        Self {
            initiated_at: Instant::now(),
            started_at: None,
            head_parsed_at: None,
            complete_at: None,
            current: None,
        }
    }

    fn record(&mut self, status: LoadStatus) {
        let now = Instant::now();
        self.current = Some(status);
        match status {
            LoadStatus::Started => self.started_at = Some(now),
            LoadStatus::HeadParsed => self.head_parsed_at = Some(now),
            LoadStatus::Complete => self.complete_at = Some(now),
        }
    }

    /// Returns `true` once `LoadStatus::Complete` has been observed.
    pub fn is_complete(&self) -> bool {
        self.complete_at.is_some()
    }

    /// Has the given `status` been reached (or surpassed)?
    pub fn has_reached(&self, status: LoadStatus) -> bool {
        match status {
            LoadStatus::Started => self.started_at.is_some(),
            LoadStatus::HeadParsed => self.head_parsed_at.is_some(),
            LoadStatus::Complete => self.complete_at.is_some(),
        }
    }
}

// ── WebView delegate ────────────────────────────────────────────────────────

/// Optional user callback invoked on every [`LoadStatus`] transition.
pub type LoadStatusCallback = Box<dyn Fn(LoadStatus, &PageLoadProgress)>;

/// Optional user callback invoked when a webview's content process crashes.
pub type CrashCallback = Box<dyn Fn(&CrashInfo)>;
/// Optional user callback invoked when session history changes (SPA navigation).
pub type HistoryChangedCallback = Box<dyn Fn(&[Url], usize)>;
/// Information about a content-process crash.
#[derive(Debug, Clone)]
pub struct CrashInfo {
    /// Human-readable reason for the crash (e.g. panic message).
    pub reason: String,
    /// Optional backtrace captured at crash time.
    pub backtrace: Option<String>,
    /// When the crash was observed.
    pub timestamp: Instant,
}

struct GhostWebViewDelegate {
    progress: Rc<RefCell<PageLoadProgress>>,
    on_status: Rc<RefCell<Option<LoadStatusCallback>>>,
    crash: Rc<RefCell<Option<CrashInfo>>>,
    on_crash: Rc<RefCell<Option<CrashCallback>>>,
    /// URL substring patterns to block. If any pattern matches a request URL,
    /// the request is cancelled.
    block_patterns: Rc<RefCell<Vec<String>>>,
    /// Session history entries (URLs) last reported by Servo.
    history_entries: Rc<RefCell<Vec<Url>>>,
    /// Index of the current entry in [`Self::history_entries`].
    history_current: Rc<RefCell<usize>>,
    on_history_changed: Rc<RefCell<Option<HistoryChangedCallback>>>,
    /// Resource budget for blocking heavy sub-resources.
    resource_budget: Rc<RefCell<ResourceBudget>>,
    /// Counter: total sub-resource requests blocked by budget rules.
    resources_blocked: Arc<AtomicU64>,
    /// Counter: total bytes saved by blocking (estimated from Content-Length).
    bytes_saved: Arc<AtomicU64>,
}

impl WebViewDelegate for GhostWebViewDelegate {
    fn notify_load_status_changed(&self, _webview: WebView, status: LoadStatus) {
        {
            let mut p = self.progress.borrow_mut();
            p.record(status);
        }
        if let Some(cb) = self.on_status.borrow().as_ref() {
            cb(status, &self.progress.borrow());
        }
    }

    fn notify_crashed(&self, _webview: WebView, reason: String, backtrace: Option<String>) {
        let info = CrashInfo {
            reason,
            backtrace,
            timestamp: Instant::now(),
        };
        if let Some(cb) = self.on_crash.borrow().as_ref() {
            cb(&info);
        }
        *self.crash.borrow_mut() = Some(info);
    }

    fn load_web_resource(&self, _webview: WebView, load: WebResourceLoad) {
        let req = load.request();
        let url_str = req.url.as_str();

        // 1. User-defined URL block patterns.
        let patterns = self.block_patterns.borrow();
        if patterns.iter().any(|p| url_str.contains(p.as_str())) {
            self.cancel_load(load);
            return;
        }
        drop(patterns);

        // 2. Resource budget filtering (TSK-5.7).
        // Skip main-frame document loads — always allow navigation.
        if !req.is_for_main_frame {
            let budget = self.resource_budget.borrow();

            // Check Accept header for resource type detection.
            let accept = req.headers.get("accept")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            // Image blocking.
            if budget.skip_images && (
                accept.starts_with("image/") ||
                is_image_url(url_str)
            ) {
                self.cancel_load(load);
                return;
            }

            // Font blocking.
            if budget.skip_fonts && is_font_url(url_str) {
                self.cancel_load(load);
                return;
            }

            // Media blocking.
            if budget.skip_media && is_media_url(url_str) {
                self.cancel_load(load);
                return;
            }

            // Stylesheet blocking.
            if budget.skip_stylesheets && (
                accept.starts_with("text/css") ||
                is_stylesheet_url(url_str)
            ) {
                self.cancel_load(load);
                return;
            }

            // Max resource size (Content-Length check).
            if budget.max_resource_bytes > 0 {
                if let Some(cl) = req.headers.get("content-length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    if cl > budget.max_resource_bytes {
                        self.bytes_saved.fetch_add(cl, Ordering::Relaxed);
                        self.cancel_load(load);
                        return;
                    }
                }
            }
        }
    }

    fn notify_history_changed(&self, _webview: WebView, entries: Vec<Url>, current: usize) {
        *self.history_entries.borrow_mut() = entries.clone();
        *self.history_current.borrow_mut() = current;
        if let Some(cb) = self.on_history_changed.borrow().as_ref() {
            cb(&entries, current);
        }
    }
}

impl GhostWebViewDelegate {
    /// Cancel a web resource load and bump the blocked counter.
    fn cancel_load(&self, load: WebResourceLoad) {
        self.resources_blocked.fetch_add(1, Ordering::Relaxed);
        let resp = servo::WebResourceResponse::new(load.request().url.clone());
        load.intercept(resp).cancel();
    }
}

/// Check if a URL path looks like an image resource.
fn is_image_url(url: &str) -> bool {
    // Strip query/fragment, then check extension.
    let path = url_path_lower(url);
    matches!(
        path_extension(&path),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "ico" | "avif" | "bmp" | "tiff"
    )
}

/// Check if a URL path looks like a font resource.
fn is_font_url(url: &str) -> bool {
    let path = url_path_lower(url);
    matches!(path_extension(&path), "woff" | "woff2" | "ttf" | "otf" | "eot")
}

/// Check if a URL path looks like a media resource.
fn is_media_url(url: &str) -> bool {
    let path = url_path_lower(url);
    matches!(
        path_extension(&path),
        "mp4" | "webm" | "mp3" | "ogg" | "wav" | "flac" | "m3u8" | "m4a"
            | "avi" | "mkv" | "mov" | "aac"
    )
}

/// Check if a URL path looks like a stylesheet.
fn is_stylesheet_url(url: &str) -> bool {
    let path = url_path_lower(url);
    path_extension(&path) == "css"
}

/// Extract the lowercase URL path, stripping query and fragment.
fn url_path_lower(url: &str) -> String {
    let without_scheme = url.find("://")
        .map(|i| &url[i + 3..])
        .unwrap_or(url);
    let path_start = without_scheme.find('/').unwrap_or(without_scheme.len());
    let path = &without_scheme[path_start..];
    let end = path.find(['?', '#']).unwrap_or(path.len());
    path[..end].to_ascii_lowercase()
}

/// Get the file extension from a lowercase path.
fn path_extension(path: &str) -> &str {
    path.rsplit_once('.')
        .map(|(_, ext)| ext)
        .unwrap_or("")
}

// ── GhostEngine ─────────────────────────────────────────────────────────────

/// A headless browser engine powered by Servo.
///
/// ```no_run
/// use ghost_core::{GhostEngine, GhostEngineConfig};
///
/// let engine = GhostEngine::new(GhostEngineConfig::default()).unwrap();
/// let webview = engine.new_webview("https://example.com").unwrap();
/// engine.load_and_wait(&webview).unwrap();
/// println!("title = {:?}", webview.page_title());
/// ```
pub struct GhostEngine {
    servo: Servo,
    rendering_context: Rc<dyn RenderingContext>,
    waker: HeadlessWaker,
    config: GhostEngineConfig,
    /// Time spent initialising Servo (TSK-5.5).
    init_duration: Duration,
}

impl GhostEngine {
    /// Create a new headless Servo instance.
    pub fn new(config: GhostEngineConfig) -> Result<Self, GhostError> {
        let init_start = Instant::now();
        ensure_runtime_initialized(&config);

        let waker = HeadlessWaker::new();

        let mut prefs = Preferences::default();
        if let Some(ua) = &config.user_agent {
            prefs.user_agent = ua.clone();
        }
        // TSK-5.8: Connection and cache tuning.
        prefs.network_connection_timeout = config.connection_timeout.as_secs();
        prefs.network_http_cache_disabled = !config.http_cache_enabled;
        prefs.network_http_cache_size = config.http_cache_size;

        let servo = ServoBuilder::default()
            .opts(Opts::default())
            .preferences(prefs)
            .event_loop_waker(Box::new(waker.clone()))
            .build();

        servo.setup_logging();

        let size = dpi::PhysicalSize::new(config.viewport_width, config.viewport_height);
        let rendering_context: Rc<dyn RenderingContext> = Rc::new(
            SoftwareRenderingContext::new(size)
                .map_err(|e| GhostError::Init(format!("Failed to create rendering context: {e:?}")))?,
        );

        let init_duration = init_start.elapsed();

        Ok(Self {
            servo,
            rendering_context,
            waker,
            config,
            init_duration,
        })
    }

    /// Create a WebView targeting the given URL.
    pub fn new_webview(&self, url: &str) -> Result<GhostWebView, GhostError> {
        self.new_webview_with_options(url, &[])
    }

    /// Create a WebView with URL block patterns for network filtering.
    ///
    /// Any HTTP/HTTPS request whose URL contains one of the `block_patterns`
    /// substrings will be cancelled before reaching the network.
    pub fn new_webview_with_options(
        &self,
        url: &str,
        block_patterns: &[String],
    ) -> Result<GhostWebView, GhostError> {
        let parsed =
            Url::parse(url).map_err(|e| GhostError::Navigation(format!("Invalid URL: {e}")))?;

        let progress = Rc::new(RefCell::new(PageLoadProgress::new()));
        let on_status: Rc<RefCell<Option<LoadStatusCallback>>> = Rc::new(RefCell::new(None));
        let crash: Rc<RefCell<Option<CrashInfo>>> = Rc::new(RefCell::new(None));
        let on_crash: Rc<RefCell<Option<CrashCallback>>> = Rc::new(RefCell::new(None));
        let block_pats: Rc<RefCell<Vec<String>>> =
            Rc::new(RefCell::new(block_patterns.to_vec()));
        let history_entries: Rc<RefCell<Vec<Url>>> = Rc::new(RefCell::new(Vec::new()));
        let history_current: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
        let on_history_changed: Rc<RefCell<Option<HistoryChangedCallback>>> =
            Rc::new(RefCell::new(None));
        let resource_budget: Rc<RefCell<ResourceBudget>> =
            Rc::new(RefCell::new(self.config.resource_budget.clone()));
        let resources_blocked = Arc::new(AtomicU64::new(0));
        let bytes_saved = Arc::new(AtomicU64::new(0));
        let delegate = Rc::new(GhostWebViewDelegate {
            progress: progress.clone(),
            on_status: on_status.clone(),
            crash: crash.clone(),
            on_crash: on_crash.clone(),
            block_patterns: block_pats.clone(),
            history_entries: history_entries.clone(),
            history_current: history_current.clone(),
            on_history_changed: on_history_changed.clone(),
            resource_budget: resource_budget.clone(),
            resources_blocked: resources_blocked.clone(),
            bytes_saved: bytes_saved.clone(),
        });
        let user_content = Rc::new(UserContentManager::new(&self.servo));

        let webview = WebViewBuilder::new(&self.servo, self.rendering_context.clone())
            .url(parsed)
            .delegate(delegate)
            .user_content_manager(user_content)
            .build();

        webview.focus();

        Ok(GhostWebView {
            inner: webview,
            progress,
            on_status,
            crash,
            on_crash,
            block_patterns: block_pats,
            history_entries,
            history_current,
            on_history_changed,
            resource_budget,
            resources_blocked,
            bytes_saved,
        })
    }

    /// Spin the Servo event loop once.
    pub fn spin(&self) {
        self.servo.spin_event_loop();
    }

    /// Spin the event loop until the webview reaches the given
    /// [`LoadStatus`] or the configured timeout is reached.
    pub fn wait_until(
        &self,
        webview: &GhostWebView,
        target: LoadStatus,
    ) -> Result<(), GhostError> {
        let deadline = Instant::now() + self.config.load_timeout;

        while !webview.progress.borrow().has_reached(target) {
            if let Some(info) = webview.crash.borrow().as_ref() {
                return Err(GhostError::Crashed {
                    reason: info.reason.clone(),
                    backtrace: info.backtrace.clone(),
                });
            }
            if Instant::now() >= deadline {
                return Err(GhostError::Timeout);
            }
            self.waker.sleep(Duration::from_millis(5));
            self.servo.spin_event_loop();
            self.waker.clear();
        }

        Ok(())
    }

    /// Spin the event loop until `LoadStatus::Complete`, then settle.
    ///
    /// This is a convenience wrapper around [`Self::wait_until`] +
    /// [`Self::settle`].
    pub fn load_and_wait(&self, webview: &GhostWebView) -> Result<(), GhostError> {
        self.wait_until(webview, LoadStatus::Complete)?;
        self.settle();
        Ok(())
    }

    /// Evaluate a JavaScript expression in the given webview and return the
    /// result, spinning the event loop until the callback fires.
    ///
    /// The script should be a single expression (not a statement). Servo
    /// evaluates it in the document's global scope and serialises the return
    /// value into a [`JSValue`].
    pub fn evaluate_js(
        &self,
        webview: &GhostWebView,
        script: &str,
    ) -> Result<JSValue, GhostError> {
        // Wrap the entire JS evaluation + event-loop spin in catch_unwind
        // so that Servo panics (e.g. from unsupported DOM APIs) don't
        // tear down the host process.
        let this = AssertUnwindSafe(self);
        let wv = AssertUnwindSafe(webview);

        match catch_unwind(move || {
            let result: Rc<RefCell<Option<Result<JSValue, servo::JavaScriptEvaluationError>>>> =
                Rc::new(RefCell::new(None));
            let result_cb = result.clone();

            wv.servo_webview().evaluate_javascript(script, move |res| {
                *result_cb.borrow_mut() = Some(res);
            });

            let deadline = Instant::now() + this.config.load_timeout;
            loop {
                this.servo.spin_event_loop();
                this.waker.clear();

                if result.borrow().is_some() {
                    break;
                }
                if wv.has_crashed() {
                    let info = wv
                        .crash_info()
                        .map(|c| c.reason)
                        .unwrap_or_else(|| "unknown".into());
                    return Err(GhostError::Crashed {
                        reason: info,
                        backtrace: None,
                    });
                }
                if Instant::now() >= deadline {
                    return Err(GhostError::Timeout);
                }
                this.waker.sleep(Duration::from_millis(5));
            }

            result
                .take()
                .unwrap()
                .map_err(|e| GhostError::JavaScript(format!("{e:?}")))
        }) {
            Ok(inner) => inner,
            Err(payload) => {
                let msg = if let Some(s) = payload.downcast_ref::<String>() {
                    s.clone()
                } else if let Some(s) = payload.downcast_ref::<&str>() {
                    (*s).to_string()
                } else {
                    "unknown panic".to_string()
                };
                Err(GhostError::Panic(msg))
            },
        }
    }

    /// Block until `document.querySelector(selector)` returns a visible
    /// element, or the given `timeout` expires.
    ///
    /// This is the Playwright-style `wait_for_selector` primitive needed
    /// for SPAs that lazy-render content after the initial page load.
    /// Visibility checks match the extraction filter: `display:none`,
    /// `visibility:hidden`, and zero-area elements are not considered.
    pub fn wait_for_selector(
        &self,
        webview: &GhostWebView,
        selector: &str,
        timeout: Duration,
    ) -> Result<(), GhostError> {
        let escaped = js_escape_string(selector);
        let script = format!(
            r#"(function(){{
                var el = document.querySelector({escaped});
                if (!el) return false;
                var cs = window.getComputedStyle(el);
                if (cs.display==='none'||cs.visibility==='hidden') return false;
                if (parseFloat(cs.opacity)===0) return false;
                var r = el.getBoundingClientRect();
                if (r.width===0&&r.height===0) return false;
                return true;
            }})()"#
        );

        let deadline = Instant::now() + timeout;
        loop {
            let js_result = self.evaluate_js(webview, &script)?;
            if matches!(js_result, JSValue::Boolean(true)) {
                return Ok(());
            }
            if webview.has_crashed() {
                let info = webview
                    .crash_info()
                    .map(|c| c.reason)
                    .unwrap_or_else(|| "unknown".into());
                return Err(GhostError::Crashed {
                    reason: info,
                    backtrace: None,
                });
            }
            if Instant::now() >= deadline {
                return Err(GhostError::Timeout);
            }
            self.waker.sleep(Duration::from_millis(50));
            self.servo.spin_event_loop();
            self.waker.clear();
        }
    }

    /// Take a screenshot of the current viewport and return PNG-encoded bytes.
    ///
    /// Spins the event loop until the compositor delivers the frame, then
    /// encodes the RGBA image as PNG. Returns the raw PNG data suitable for
    /// base64-encoding or writing to a file.
    pub fn take_screenshot_png(
        &self,
        webview: &GhostWebView,
    ) -> Result<Vec<u8>, GhostError> {
        let this = AssertUnwindSafe(self);
        let wv = AssertUnwindSafe(webview);

        match catch_unwind(move || {
            let result: Rc<RefCell<Option<Result<image::RgbaImage, String>>>> =
                Rc::new(RefCell::new(None));
            let result_cb = result.clone();

            wv.servo_webview().take_screenshot(None, move |res| {
                *result_cb.borrow_mut() = Some(res.map_err(|e| format!("{e:?}")));
            });

            let deadline = Instant::now() + this.config.load_timeout;
            loop {
                wv.servo_webview().paint();
                this.servo.spin_event_loop();
                this.waker.clear();

                if result.borrow().is_some() {
                    break;
                }
                if wv.has_crashed() {
                    let info = wv
                        .crash_info()
                        .map(|c| c.reason)
                        .unwrap_or_else(|| "unknown".into());
                    return Err(GhostError::Crashed {
                        reason: info,
                        backtrace: None,
                    });
                }
                if Instant::now() >= deadline {
                    return Err(GhostError::Timeout);
                }
                this.waker.sleep(Duration::from_millis(16));
            }

            let rgba = result
                .take()
                .unwrap()
                .map_err(|msg| GhostError::Init(format!("screenshot failed: {msg}")))?;

            let mut cursor = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(rgba)
                .write_to(&mut cursor, image::ImageFormat::Png)
                .map_err(|e| GhostError::Init(format!("PNG encode failed: {e}")))?;

            Ok(cursor.into_inner())
        }) {
            Ok(inner) => inner,
            Err(payload) => {
                let msg = if let Some(s) = payload.downcast_ref::<String>() {
                    s.clone()
                } else if let Some(s) = payload.downcast_ref::<&str>() {
                    (*s).to_string()
                } else {
                    "unknown panic".to_string()
                };
                Err(GhostError::Panic(msg))
            },
        }
    }

    /// Keep spinning the Servo event loop until either
    /// [`GhostEngineConfig::settle_timeout`] expires **or** the waker has
    /// been quiet for [`GhostEngineConfig::quiet_period`], whichever comes
    /// first. This gives setTimeout / fetch / Promise callbacks time to run
    /// after the initial page load.
    pub fn settle(&self) {
        if self.config.settle_timeout.is_zero() {
            return;
        }

        let settle_deadline = Instant::now() + self.config.settle_timeout;
        let mut last_activity = Instant::now();

        loop {
            let now = Instant::now();
            if now >= settle_deadline {
                break;
            }
            if now.duration_since(last_activity) >= self.config.quiet_period {
                break;
            }

            let woken = self.waker.sleep(Duration::from_millis(5));
            self.servo.spin_event_loop();
            if woken {
                last_activity = Instant::now();
            }
            self.waker.clear();
        }
    }
}

// ── GhostWebView ────────────────────────────────────────────────────────────

/// Handle to a headless browser tab managed by [`GhostEngine`].
pub struct GhostWebView {
    inner: WebView,
    progress: Rc<RefCell<PageLoadProgress>>,
    on_status: Rc<RefCell<Option<LoadStatusCallback>>>,
    crash: Rc<RefCell<Option<CrashInfo>>>,
    on_crash: Rc<RefCell<Option<CrashCallback>>>,
    block_patterns: Rc<RefCell<Vec<String>>>,
    history_entries: Rc<RefCell<Vec<Url>>>,
    history_current: Rc<RefCell<usize>>,
    on_history_changed: Rc<RefCell<Option<HistoryChangedCallback>>>,
    resource_budget: Rc<RefCell<ResourceBudget>>,
    resources_blocked: Arc<AtomicU64>,
    bytes_saved: Arc<AtomicU64>,
}

impl GhostWebView {
    /// The current page title, if available.
    pub fn page_title(&self) -> Option<String> {
        self.inner.page_title()
    }

    /// The current document URL, if available.
    pub fn url(&self) -> Option<Url> {
        self.inner.url()
    }

    /// Navigate to a new URL, resetting load progress tracking.
    pub fn load(&self, url: &str) -> Result<(), GhostError> {
        let parsed =
            Url::parse(url).map_err(|e| GhostError::Navigation(format!("Invalid URL: {e}")))?;
        *self.progress.borrow_mut() = PageLoadProgress::new();
        self.inner.load(parsed);
        Ok(())
    }

    /// Whether `LoadStatus::Complete` has been observed for the current page.
    pub fn is_loaded(&self) -> bool {
        self.progress.borrow().is_complete()
    }

    /// A snapshot of the current page-load progress with per-phase timestamps.
    pub fn load_progress(&self) -> PageLoadProgress {
        self.progress.borrow().clone()
    }

    /// Register a callback invoked on every [`LoadStatus`] transition.
    ///
    /// Replaces any previously registered callback. Pass `None` to clear.
    pub fn set_on_load_status(&self, cb: Option<LoadStatusCallback>) {
        *self.on_status.borrow_mut() = cb;
    }

    /// Reload the current page, resetting load progress.
    pub fn reload(&self) {
        *self.progress.borrow_mut() = PageLoadProgress::new();
        self.inner.reload();
    }

    /// Go back in session history. Returns `true` if history had an entry.
    pub fn go_back(&self) -> bool {
        if self.inner.can_go_back() {
            *self.progress.borrow_mut() = PageLoadProgress::new();
            self.inner.go_back(1);
            true
        } else {
            false
        }
    }

    /// Go forward in session history. Returns `true` if history had an entry.
    pub fn go_forward(&self) -> bool {
        if self.inner.can_go_forward() {
            *self.progress.borrow_mut() = PageLoadProgress::new();
            self.inner.go_forward(1);
            true
        } else {
            false
        }
    }

    /// The underlying Servo [`WebView`] for advanced usage.
    pub fn servo_webview(&self) -> &WebView {
        &self.inner
    }

    /// Whether the webview's content process has crashed.
    pub fn has_crashed(&self) -> bool {
        self.crash.borrow().is_some()
    }

    /// Crash information, if the webview has crashed.
    pub fn crash_info(&self) -> Option<CrashInfo> {
        self.crash.borrow().clone()
    }

    /// Register a callback invoked when the webview's content process crashes.
    ///
    /// Replaces any previously registered callback. Pass `None` to clear.
    pub fn set_on_crash(&self, cb: Option<CrashCallback>) {
        *self.on_crash.borrow_mut() = cb;
    }

    /// Replace the set of URL block patterns. Any pending navigations are not
    /// affected — only new network requests will be matched.
    pub fn set_block_patterns(&self, patterns: Vec<String>) {
        *self.block_patterns.borrow_mut() = patterns;
    }

    /// Get the current URL block patterns.
    pub fn block_patterns(&self) -> Vec<String> {
        self.block_patterns.borrow().clone()
    }

    /// The current session history entries as reported by Servo.
    ///
    /// Updated whenever `history.pushState`, `history.replaceState`, or
    /// back/forward navigation occurs. Empty until the first
    /// `notify_history_changed` event fires.
    pub fn history_entries(&self) -> Vec<Url> {
        self.history_entries.borrow().clone()
    }

    /// Index of the current entry in [`Self::history_entries`].
    pub fn history_current_index(&self) -> usize {
        *self.history_current.borrow()
    }

    /// Check whether the current URL differs from a previously observed URL.
    ///
    /// This is the primary SPA route-change detection primitive. After an
    /// action that might trigger `history.pushState` (e.g. clicking a link
    /// in a React/Vue/Angular app), compare the URL against the value
    /// captured before the action:
    ///
    /// ```ignore
    /// let before = webview.url();
    /// // … perform action …
    /// engine.settle();
    /// if webview.url_changed_since(before.as_ref()) {
    ///     // SPA navigation detected — re-extract layout
    /// }
    /// ```
    pub fn url_changed_since(&self, previous: Option<&Url>) -> bool {
        let current = self.inner.url();
        match (previous, current) {
            (Some(prev), Some(cur)) => prev != &cur,
            (None, Some(_)) => true,
            _ => false,
        }
    }

    /// Register a callback invoked when session history changes.
    ///
    /// Fires on `history.pushState`, `history.replaceState`, and
    /// back/forward traversals. The callback receives the full history
    /// entry list and the index of the current entry.
    ///
    /// Replaces any previously registered callback. Pass `None` to clear.
    pub fn set_on_history_changed(&self, cb: Option<HistoryChangedCallback>) {
        *self.on_history_changed.borrow_mut() = cb;
    }

    // ── Resource budget (TSK-5.7) ───────────────────────────────────────

    /// Replace the resource budget. Takes effect for subsequent requests.
    pub fn set_resource_budget(&self, budget: ResourceBudget) {
        *self.resource_budget.borrow_mut() = budget;
    }

    /// Get the current resource budget.
    pub fn resource_budget(&self) -> ResourceBudget {
        self.resource_budget.borrow().clone()
    }

    /// Number of sub-resource requests blocked by budget rules since this
    /// webview was created.
    pub fn resources_blocked(&self) -> u64 {
        self.resources_blocked.load(Ordering::Relaxed)
    }

    /// Estimated bytes saved by blocking sub-resources (from
    /// Content-Length headers of cancelled requests).
    pub fn bytes_saved(&self) -> u64 {
        self.bytes_saved.load(Ordering::Relaxed)
    }

    // ── Performance report (TSK-5.5) ────────────────────────────────────

    /// Build a timing breakdown for the most recent page load.
    pub fn load_timing(&self) -> LoadTiming {
        let p = self.progress.borrow();
        let nav_to_started = p.started_at
            .map(|t| t.duration_since(p.initiated_at));
        let started_to_head = match (p.started_at, p.head_parsed_at) {
            (Some(a), Some(b)) => Some(b.duration_since(a)),
            _ => None,
        };
        let head_to_complete = match (p.head_parsed_at, p.complete_at) {
            (Some(a), Some(b)) => Some(b.duration_since(a)),
            _ => None,
        };
        let total = p.complete_at
            .map(|t| t.duration_since(p.initiated_at));

        LoadTiming {
            navigation: nav_to_started,
            head_parse: started_to_head,
            subresources: head_to_complete,
            total,
        }
    }
}

// ── Performance & profiling types (TSK-5.5, TSK-5.6) ────────────────────────

/// Timing breakdown for a single page load.
#[derive(Debug, Clone, Default)]
pub struct LoadTiming {
    /// Time from navigation initiation to `LoadStatus::Started`.
    pub navigation: Option<Duration>,
    /// Time from `Started` to `HeadParsed` (HTML parsing + head evaluation).
    pub head_parse: Option<Duration>,
    /// Time from `HeadParsed` to `Complete` (sub-resources + remaining JS).
    pub subresources: Option<Duration>,
    /// Total time from initiation to `Complete`.
    pub total: Option<Duration>,
}

impl std::fmt::Display for LoadTiming {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn ms(d: Option<Duration>) -> String {
            d.map(|d| format!("{:.1}ms", d.as_secs_f64() * 1000.0))
                .unwrap_or_else(|| "-".into())
        }
        write!(
            f,
            "nav={}, head={}, sub={}, total={}",
            ms(self.navigation),
            ms(self.head_parse),
            ms(self.subresources),
            ms(self.total),
        )
    }
}

/// A snapshot of the engine's performance and resource usage.
#[derive(Debug, Clone)]
pub struct PerfReport {
    /// Time spent initialising the Servo engine.
    pub engine_init: Duration,
    /// Current process RSS in bytes (0 if unavailable).
    pub rss_bytes: u64,
    /// Load timing for the active page (if any).
    pub load_timing: Option<LoadTiming>,
    /// Number of sub-resources blocked by budget rules.
    pub resources_blocked: u64,
    /// Estimated bytes saved by resource blocking.
    pub bytes_saved: u64,
}

impl std::fmt::Display for PerfReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "engine_init: {:.1}ms", self.engine_init.as_secs_f64() * 1000.0)?;
        writeln!(f, "rss: {:.1} MB", self.rss_bytes as f64 / (1024.0 * 1024.0))?;
        if let Some(lt) = &self.load_timing {
            writeln!(f, "load: {lt}")?;
        }
        writeln!(f, "blocked: {} requests, ~{:.1} KB saved",
            self.resources_blocked,
            self.bytes_saved as f64 / 1024.0,
        )?;
        Ok(())
    }
}

impl GhostEngine {
    /// The time it took to initialise the Servo engine.
    pub fn init_duration(&self) -> Duration {
        self.init_duration
    }

    /// Build a [`PerfReport`] snapshot for the given webview.
    pub fn perf_report(&self, webview: &GhostWebView) -> PerfReport {
        PerfReport {
            engine_init: self.init_duration,
            rss_bytes: current_rss_bytes(),
            load_timing: Some(webview.load_timing()),
            resources_blocked: webview.resources_blocked(),
            bytes_saved: webview.bytes_saved(),
        }
    }
}

/// Return the current process RSS (Resident Set Size) in bytes.
///
/// Uses platform-specific APIs:
/// - macOS: `mach_task_self()` + `task_info(MACH_TASK_BASIC_INFO)`
/// - Linux: reads `/proc/self/statm`
/// - Other: returns 0
pub fn current_rss_bytes() -> u64 {
    #[cfg(target_os = "macos")]
    {
        macos_rss()
    }
    #[cfg(target_os = "linux")]
    {
        linux_rss()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0
    }
}

#[cfg(target_os = "macos")]
fn macos_rss() -> u64 {
    use std::mem;

    #[repr(C)]
    struct MachTaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time: [u32; 2],
        system_time: [u32; 2],
        policy: i32,
        suspend_count: i32,
    }

    const MACH_TASK_BASIC_INFO: u32 = 20;

    unsafe {
        #[allow(deprecated)]
        let port = libc::mach_task_self();
        let mut info: MachTaskBasicInfo = mem::zeroed();
        let mut count = (mem::size_of::<MachTaskBasicInfo>() / mem::size_of::<u32>()) as u32;
        let kr = libc::task_info(
            port,
            MACH_TASK_BASIC_INFO,
            &raw mut info as *mut i32,
            &raw mut count,
        );
        if kr == 0 { info.resident_size } else { 0 }
    }
}

#[cfg(target_os = "linux")]
fn linux_rss() -> u64 {
    // /proc/self/statm fields: size resident shared text lib data dt
    // All values are in pages.
    if let Ok(contents) = std::fs::read_to_string("/proc/self/statm") {
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as u64 };
        contents
            .split_whitespace()
            .nth(1) // resident pages
            .and_then(|s| s.parse::<u64>().ok())
            .map(|pages| pages * page_size)
            .unwrap_or(0)
    } else {
        0
    }
}

// ── Errors ──────────────────────────────────────────────────────────────────

/// Errors produced by [`GhostEngine`] operations.
#[derive(Debug)]
pub enum GhostError {
    Init(String),
    Navigation(String),
    Timeout,
    Crashed {
        reason: String,
        backtrace: Option<String>,
    },
    JavaScript(String),
    /// A Servo thread panicked during an operation. The message contains
    /// the panic payload (if it was a string).
    Panic(String),
}

impl std::fmt::Display for GhostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GhostError::Init(msg) => write!(f, "engine init failed: {msg}"),
            GhostError::Navigation(msg) => write!(f, "navigation failed: {msg}"),
            GhostError::Timeout => write!(f, "page load timed out"),
            GhostError::Crashed { reason, .. } => {
                write!(f, "content process crashed: {reason}")
            },
            GhostError::JavaScript(msg) => write!(f, "javascript evaluation failed: {msg}"),
            GhostError::Panic(msg) => write!(f, "servo panicked: {msg}"),
        }
    }
}

impl std::error::Error for GhostError {}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Escape a Rust string into a JS string literal (including quotes).
/// Prevents injection when embedding user-supplied values in JS code.
fn js_escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // U+2028 / U+2029 are line terminators in JS (pre-ES2019).
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if c < '\x20' => {
                let _ = std::fmt::Write::write_fmt(
                    &mut out,
                    format_args!("\\u{:04x}", c as u32),
                );
            },
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ── Legacy compat re-exports (from TSK-1.6) ────────────────────────────────

pub const VERSION: &str = servoshell::VERSION;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sane_values() {
        let cfg = GhostEngineConfig::default();
        assert_eq!(cfg.viewport_width, 1920);
        assert_eq!(cfg.viewport_height, 1080);
        assert_eq!(cfg.load_timeout, Duration::from_secs(30));
        assert_eq!(cfg.settle_timeout, Duration::from_secs(2));
        assert_eq!(cfg.quiet_period, Duration::from_millis(500));
    }

    #[test]
    fn page_load_progress_lifecycle() {
        let mut p = PageLoadProgress::new();
        assert!(!p.is_complete());
        assert!(!p.has_reached(LoadStatus::Started));

        p.record(LoadStatus::Started);
        assert!(p.has_reached(LoadStatus::Started));
        assert!(!p.has_reached(LoadStatus::HeadParsed));
        assert!(p.started_at.is_some());

        p.record(LoadStatus::HeadParsed);
        assert!(p.has_reached(LoadStatus::HeadParsed));
        assert!(!p.is_complete());
        assert!(p.head_parsed_at.is_some());

        p.record(LoadStatus::Complete);
        assert!(p.is_complete());
        assert!(p.has_reached(LoadStatus::Complete));
        assert!(p.complete_at.is_some());
    }
}