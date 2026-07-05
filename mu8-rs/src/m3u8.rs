use regex::Regex;
use url::Url;

/// M3U8 解析结果
#[derive(Debug, Clone)]
pub struct M3u8ParseResult {
    pub segments: Vec<String>,
    pub key_method: Option<String>,
    pub key_uri: Option<String>,
    pub key_iv: Option<String>,
}

impl M3u8ParseResult {
    pub fn empty() -> Self {
        M3u8ParseResult {
            segments: vec![],
            key_method: None,
            key_uri: None,
            key_iv: None,
        }
    }
}

/// 解析 M3U8 文件内容
/// 对应 Python: Downloader.m3u8_parse()
pub fn parse_m3u8(content: &str, base_url: &str) -> M3u8ParseResult {
    let mut segments: Vec<String> = Vec::new();
    let mut key_method: Option<String> = None;
    let mut key_uri: Option<String> = None;
    let mut key_iv: Option<String> = None;
    let mut stream_infs: Vec<(u64, String)> = Vec::new(); // (bandwidth, url)

    let key_re = Regex::new(r"METHOD=([^,]+)").unwrap();
    let uri_re = Regex::new(r#"URI="([^"]+)""#).unwrap();
    let iv_re = Regex::new(r"IV=([^,]+)").unwrap();
    let bandwidth_re = Regex::new(r"BANDWIDTH=(\d+)").unwrap();

    let lines: Vec<&str> = content.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();

        if line.starts_with("#EXT-X-KEY") {
            if let Some(caps) = key_re.captures(line) {
                key_method = Some(caps[1].to_string());
            }
            if let Some(caps) = uri_re.captures(line) {
                let uri = caps[1].to_string();
                key_uri = Some(resolve_url(&uri, base_url));
            }
            if let Some(caps) = iv_re.captures(line) {
                key_iv = Some(caps[1].to_string());
            }
        } else if line.starts_with("#EXT-X-STREAM-INF") {
            let bandwidth = bandwidth_re
                .captures(line)
                .and_then(|c| c[1].parse::<u64>().ok())
                .unwrap_or(0);
            if i + 1 < lines.len() {
                let next_line = lines[i + 1].trim();
                if next_line.to_lowercase().ends_with(".m3u8") {
                    let url = resolve_url(next_line, base_url);
                    log::info!("找到 M3U8 链接: {}, 带宽: {}", url, bandwidth);
                    stream_infs.push((bandwidth, url));
                }
            }
        } else if line.starts_with("#EXTINF") {
            if i + 1 < lines.len() {
                let seg_url = lines[i + 1].trim();
                let full_url = resolve_url(seg_url, base_url);
                segments.push(full_url);
            }
        }
        i += 1;
    }

    // 如果有 variant playlist，选择最高画质
    if !stream_infs.is_empty() {
        stream_infs.sort_by(|a, b| b.0.cmp(&a.0)); // 按带宽降序
        let highest_url = &stream_infs[0].1;
        log::info!("选择最高画质 M3U8 链接: {}", highest_url);
        // 递归解析 —— 由调用者负责获取内容并再次调用 parse_m3u8
        return M3u8ParseResult {
            segments: vec![highest_url.clone()],
            key_method: Some("__STREAM_INF__".to_string()), // 标记需要递归
            key_uri: None,
            key_iv: None,
        };
    }

    M3u8ParseResult {
        segments,
        key_method,
        key_uri,
        key_iv,
    }
}

/// 检查是否是 variant playlist（包含 #EXT-X-STREAM-INF）
#[allow(dead_code)]
pub fn is_variant_playlist(content: &str) -> bool {
    content.contains("#EXT-X-STREAM-INF")
}

/// 解析相对 URL 为绝对 URL
pub fn resolve_url(uri: &str, base_url: &str) -> String {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        return uri.to_string();
    }
    if let Ok(base) = Url::parse(base_url) {
        if let Ok(joined) = base.join(uri) {
            return joined.to_string();
        }
    }
    // fallback: 简单拼接
    let base_dir = base_url.rsplit_once('/').map(|(d, _)| d).unwrap_or(base_url);
    format!("{}/{}", base_dir, uri.trim_start_matches('/'))
}
