use rand::Rng;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, Method, Response,
};
use std::time::Duration;

// ============================================================
// User-Agent 列表
// ============================================================
const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36 Edg/135.0.0.0",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:120.0) Gecko/20100101 Firefox/120.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0",
    "Mozilla/5.0 (iPad; CPU OS 17_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.1 Mobile/15E148 Safari/604.1",
];

// ============================================================
// AxiosError — 自定义错误类型
// ============================================================
#[derive(Debug)]
pub struct AxiosError {
    pub message: String,
    pub status_code: Option<u16>,
}

impl std::fmt::Display for AxiosError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(code) = self.status_code {
            write!(f, "AxiosError: {} (status_code={})", self.message, code)
        } else {
            write!(f, "AxiosError: {}", self.message)
        }
    }
}

impl std::error::Error for AxiosError {}

impl From<reqwest::Error> for AxiosError {
    fn from(e: reqwest::Error) -> Self {
        AxiosError {
            message: e.to_string(),
            status_code: e.status().map(|s| s.as_u16()),
        }
    }
}

// ============================================================
// Config — 请求头配置
// ============================================================
fn random_ua() -> String {
    let mut rng = rand::thread_rng();
    USER_AGENTS[rng.gen_range(0..USER_AGENTS.len())].to_string()
}

fn common_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("Accept", HeaderValue::from_static(
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7"
    ));
    headers.insert("Accept-Encoding", HeaderValue::from_static("gzip, deflate, br, zstd"));
    headers.insert("Accept-Language", HeaderValue::from_static("zh,zh-TW;q=0.9,en-US;q=0.8,en;q=0.7,zh-CN;q=0.6"));
    headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    headers.insert("Referer", HeaderValue::from_static("https://www.google.com/"));
    headers.insert("Upgrade-Insecure-Requests", HeaderValue::from_static("1"));
    headers.insert("Sec-Fetch-User", HeaderValue::from_static("?1"));
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("document"));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("navigate"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("cross-site"));
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
    headers
}

// ============================================================
// Request — HTTP 客户端（对应 Python axios.Request）
// ============================================================
pub struct Request {
    client: Client,
}

impl Request {
    pub fn new() -> Result<Self, reqwest::Error> {
        let mut headers = common_headers();
        headers.insert("User-Agent", HeaderValue::from_str(&random_ua()).unwrap());

        let client = Client::builder()
            .default_headers(headers)
            .cookie_store(true)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(77)
            .build()?;

        Ok(Request { client })
    }

    /// 带重试的请求发送
    /// 对应 Python: Retry(total=5, connect=3, read=3, backoff_factor=0.3)
    async fn send_with_retry(
        &self,
        method: Method,
        url: &str,
        extra_headers: Option<HeaderMap>,
    ) -> Result<Response, AxiosError> {
        const MAX_RETRIES: u32 = 5;
        const BACKOFF: f64 = 0.3;
        const STATUS_RETRY: [u16; 5] = [429, 500, 502, 503, 504];

        for attempt in 0..=MAX_RETRIES {
            let mut req = match method {
                Method::GET => self.client.get(url),
                Method::HEAD => self.client.head(url),
                _ => self.client.get(url),
            };

            if let Some(ref h) = extra_headers {
                req = req.headers(h.clone());
            }

            // reqwest 的 stream 模式需要单独处理
            // 非 stream 模式直接用 send()
            match req.send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    if STATUS_RETRY.contains(&status) && attempt < MAX_RETRIES {
                        let delay = BACKOFF * 2_f64.powi(attempt as i32);
                        tokio::time::sleep(Duration::from_secs_f64(delay)).await;
                        continue;
                    }
                    return Ok(resp);
                }
                Err(e) => {
                    if attempt < MAX_RETRIES {
                        let delay = BACKOFF * 2_f64.powi(attempt as i32);
                        log::warn!(
                            "Retrying ({}/{}) after error: {} — waiting {:.1}s",
                            attempt + 1,
                            MAX_RETRIES,
                            e,
                            delay
                        );
                        tokio::time::sleep(Duration::from_secs_f64(delay)).await;
                        continue;
                    }
                    return Err(AxiosError::from(e));
                }
            }
        }
        unreachable!()
    }

    pub async fn head(&self, url: &str) -> Result<Response, AxiosError> {
        self.send_with_retry(Method::HEAD, url, None).await
    }

    pub async fn get(&self, url: &str) -> Result<Response, AxiosError> {
        self.send_with_retry(Method::GET, url, None).await
    }

    #[allow(dead_code)]
    pub async fn get_with_headers(
        &self,
        url: &str,
        headers: HeaderMap,
    ) -> Result<Response, AxiosError> {
        self.send_with_retry(Method::GET, url, Some(headers))
            .await
    }

    /// 获取底层 reqwest::Client 的引用，供流式下载使用
    pub fn inner(&self) -> &Client {
        &self.client
    }
}
