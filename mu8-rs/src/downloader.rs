use crate::http::Request;
use crate::m3u8::{self, M3u8ParseResult};
use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};
use aes::Aes128;
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use reqwest::header::HeaderMap;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, mpsc};
use url::Url;

// ============================================================
// MIME 类型 → 文件扩展名
// ============================================================
fn response_type_map() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("application/vnd.apple.mpegurl", ".m3u8"),
        ("audio/mpegurl", ".m3u8"),
        ("application/x-mpegurl", ".m3u8"),
        ("text/css", ".css"),
        ("text/javascript", ".js"),
        ("text/csv", ".csv"),
        ("text/calendar", ".ics"),
        ("text/html", ".html"),
        ("application/json", ".json"),
        ("application/xml", ".xml"),
        ("text/plain", ".text"),
        ("application/pdf", ".pdf"),
        ("application/msword", ".doc"),
        ("application/vnd.openxmlformats-officedocument.wordprocessingml.document", ".docx"),
        ("application/vnd.ms-excel", ".xls"),
        ("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", ".xlsx"),
        ("application/vnd.ms-powerpoint", ".ppt"),
        ("application/vnd.openxmlformats-officedocument.presentationml.presentation", ".pptx"),
        ("application/zip", ".zip"),
        ("application/x-rar-compressed", ".rar"),
        ("application/x-7z-compressed", ".7z"),
        ("application/x-tar", ".tar"),
        ("application/gzip", ".gz"),
        ("application/x-bzip2", ".bzip2"),
        ("audio/mpeg", ".mp3"),
        ("video/mp4", ".mp4"),
        ("video/x-msvideo", ".avi"),
        ("video/x-flv", ".flv"),
        ("video/quicktime", ".mov"),
        ("video/x-ms-wmv", ".wmv"),
        ("image/jpeg", ".jpeg"),
        ("image/png", ".png"),
        ("image/gif", ".gif"),
        ("image/svg+xml", ".svg"),
        ("image/webp", ".webp"),
        ("image/x-icon", ".ico"),
        ("application/octet-stream", ".exe"),
        ("application/x-msdownload", ".msi"),
        ("application/vnd.android.package-archive", ".apk"),
        ("audio/wav", ".wav"),
        ("video/mpeg", ".mpeg"),
        ("audio/ogg", ".ogg"),
        ("video/mp2t", ".ts"),
    ])
}

// ============================================================
// 文件信息
// ============================================================
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub url: String,
    pub file_name: Option<String>,
    pub file_type: Option<String>,
    pub file_size: u64,
}

// ============================================================
// Downloader
// ============================================================
pub struct Downloader {
    out_dir: PathBuf,
    threads: usize,
    #[allow(dead_code)]
    chunk_size: usize,
    goon: bool,
    client: Request,
    response_type: HashMap<&'static str, &'static str>,
}

impl Downloader {
    pub fn new(
        out_dir: Option<&str>,
        threads: Option<usize>,
        chunk_size: Option<usize>,
        goon: bool,
    ) -> Result<Self, anyhow::Error> {
        let out_dir = PathBuf::from(out_dir.unwrap_or("./temp_download"));
        std::fs::create_dir_all(&out_dir)?;
        let out_dir = std::path::absolute(&out_dir)?;

        let cpu_half = num_cpus::get() / 2;
        let threads = threads.unwrap_or(cpu_half).min(cpu_half).min(16).max(1);

        let chunk_size = if let Some(cs) = chunk_size {
            if cs > 0 { cs } else { 500 * 1024 }
        } else {
            500 * 1024
        };

        let client = Request::new()?;

        Ok(Downloader {
            out_dir,
            threads,
            chunk_size,
            goon,
            client,
            response_type: response_type_map(),
        })
    }

    // ============================================================
    // read_url_header_content — 通过文件头魔数检测类型
    // ============================================================
    pub async fn read_url_header_content(&self, url: &str) -> Option<String> {
        let resp = match self.client.get(url).await {
            Ok(r) => r,
            Err(e) => {
                log::error!("判断文件真实类型失败: {}", e);
                return None;
            }
        };

        let bytes = match resp.bytes().await {
            Ok(b) => b,
            Err(_) => return None,
        };

        let header = &bytes[..bytes.len().min(64)];

        if header.starts_with(b"#EXTM3U") {
            Some(".m3u8".into())
        } else if header.starts_with(b"<!DOCTYPE html>") || header.starts_with(b"<!doctype html>") {
            Some(".html".into())
        } else if header.starts_with(b"\xFF\xD8\xFF") {
            Some(".jpeg".into())
        } else if header.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(".png".into())
        } else if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
            Some(".gif".into())
        } else if header.starts_with(b"\x00\x00\x00\x14ftypisom") {
            Some(".mp4".into())
        } else if header.starts_with(b"RIFF") {
            Some(".avi".into())
        } else if header.starts_with(b"FLV\x01") {
            Some(".flv".into())
        } else if header.starts_with(b"\x00\x00\x00\x14ftypqt  ") {
            Some(".mov".into())
        } else if header.starts_with(b"0&\xb2u\x8e\x66\xcf\x11\xa6\xd9\x00\xaa\x00b\xce\x6c") {
            Some(".wmv".into())
        } else if header.starts_with(b"\x00\x00\x01\xb3") {
            Some(".mpeg".into())
        } else if header.starts_with(b"\x00\x00\x00\x20ftypM4A ") {
            Some(".m4a".into())
        } else if header.starts_with(b"ID3") {
            Some(".mp3".into())
        } else if header.starts_with(b"PK\x03\x04") {
            Some(".zip".into())
        } else if header.starts_with(b"Rar!\x1a\x07\x00") {
            Some(".rar".into())
        } else if header.starts_with(b"7z\xbc\xaf\x27\x1c") {
            Some(".7z".into())
        } else if header.starts_with(b"\x1f\x8b\x08") {
            Some(".gz".into())
        } else if header.starts_with(b"\x42\x5a\x68") {
            Some(".bz2".into())
        } else if header.starts_with(b"\x00\x00\x00\x14ftypM4S ") {
            Some(".m4s".into())
        } else if header.starts_with(b"MZ") {
            Some(".exe".into())
        } else {
            None
        }
    }

    // ============================================================
    // get_file_info — 获取文件信息
    // ============================================================
    pub async fn get_file_info(&self, url: &str) -> Option<FileInfo> {
        let mut file_info = FileInfo {
            url: url.to_string(),
            file_name: None,
            file_type: None,
            file_size: 0,
        };

        let resp = match self.client.head(url).await {
            Ok(r) => r,
            Err(e) => {
                log::error!("获取文件信息失败: {}", e);
                return None;
            }
        };

        if resp.status().as_u16() == 200 {
            // 文件大小
            if let Some(cl) = resp.headers().get("Content-Length") {
                if let Ok(size) = cl.to_str().unwrap_or("0").parse::<u64>() {
                    file_info.file_size = size;
                }
            }

            // MIME 类型
            if let Some(ct) = resp.headers().get("Content-Type") {
                let ct_str = ct.to_str().unwrap_or("").split(';').next().unwrap_or("").to_lowercase();
                if let Some(ext) = self.response_type.get(ct_str.as_str()) {
                    file_info.file_type = Some(ext.to_string());
                }
            }

            // 通过文件头魔数修正类型（仅在 Content-Type 未匹配时使用）
            if file_info.file_type.is_none() {
                if let Some(magic_type) = self.read_url_header_content(url).await {
                    file_info.file_type = Some(magic_type);
                }
            }

            // 生成文件名
            let file_type = file_info.file_type.clone();
            let raw_name = match Url::parse(url) {
                Ok(parsed) => {
                    let path = parsed.path();
                    let name = Path::new(path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    // 去除 Windows 非法字符
                    let re = Regex::new(r#"[\\/*?:"<>|]"#).unwrap();
                    re.replace_all(&name, "").to_string()
                }
                Err(_) => String::new(),
            };

            let f_name = raw_name.trim().to_string();
            let now = chrono::Local::now();

            if !f_name.is_empty() {
                if let Some(ref ft) = file_type {
                    if f_name.ends_with(ft.as_str()) {
                        file_info.file_name = Some(f_name);
                    } else {
                        file_info.file_name = Some(format!("{}{}", f_name, ft));
                    }
                } else {
                    file_info.file_name = Some(f_name);
                }
            } else {
                let fallback = url.split('/').nth(2).unwrap_or("download");
                let re = Regex::new(r#"[\\/*?:"<>|]"#).unwrap();
                let fallback = re.replace_all(fallback, "").to_string();
                let mut name = format!("{}{}", fallback, now.format("%H%M%S"));
                if let Some(ref ft) = file_type {
                    name.push_str(ft);
                }
                file_info.file_name = Some(name);
            }

            // 重名处理
            if let Some(ref name) = file_info.file_name.clone() {
                let mut name = name.clone();
                if Path::new(&name).exists() && !self.goon {
                    name = format!("{}{}", now.format("%M%S"), name);
                    file_info.file_name = Some(name.clone());
                }
                let full_path = self.out_dir.join(&name);
                if full_path.exists() {
                    let new_name = self.out_dir
                        .join(format!("{}{}", now.format("%H%M%S"),
                            std::path::Path::new(&name).file_name().unwrap_or_default().to_string_lossy()))
                        .to_string_lossy()
                        .to_string();
                    file_info.file_name = Some(new_name);
                } else {
                    file_info.file_name = Some(full_path.to_string_lossy().to_string());
                }
            }

            Some(file_info)
        } else {
            log::error!("获取文件信息失败: HTTP {}", resp.status());
            None
        }
    }

    // ============================================================
    // html_parse — 解析 HTML 页面提取媒体链接
    // ============================================================
    pub async fn html_parse(&self, url: &str) -> HashSet<String> {
        let resp = match self.client.get(url).await {
            Ok(r) => r,
            Err(e) => {
                log::error!("解析 HTML 页面 {} 出错: {}", url, e);
                return HashSet::new();
            }
        };

        let html = match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                log::error!("读取 HTML 页面 {} 出错: {}", url, e);
                return HashSet::new();
            }
        };

        let document = scraper::Html::parse_document(&html);
        let base_url = match url.rsplit_once('/') {
            Some((base, _)) => format!("{}/", base),
            None => url.to_string(),
        };

        let mut links = HashSet::new();
        let m3u8_re = Regex::new(
            r"https?://[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/[^?]+\.m3u8(\?[a-zA-Z0-9_=&-]+)?"
        ).unwrap();

        // 遍历 HTML 属性
        let attrs = ["href", "src", "data-src", "data-url", "data-file", "data-config", "data-stream", "data-m3u8"];
        let selector_str = attrs.iter().map(|a| format!("[{}]", a)).collect::<Vec<_>>().join(", ");

        if let Ok(selector) = scraper::Selector::parse(&selector_str) {
            for elem in document.select(&selector) {
                for attr in &attrs {
                    if let Some(val) = elem.value().attr(attr) {
                        let val = val.replace('\\', "");
                        if let Some(m) = m3u8_re.find(&val) {
                            links.insert(m.as_str().to_string());
                        }
                    }
                }
            }
        }

        // 从 script 内容中提取链接
        let media_re = Regex::new(
            r#"(https?://[^\s"']+\.(?:mp4|mp3|m3u8))|([^\s"']+\.(?:mp4|mp3|m3u8))"#
        ).unwrap();

        if let Ok(script_sel) = scraper::Selector::parse("script") {
            for elem in document.select(&script_sel) {
                let text = elem.text().collect::<Vec<_>>().join("");
                for caps in media_re.captures_iter(&text) {
                    let full_url = if let Some(abs) = caps.get(1) {
                        abs.as_str().to_string()
                    } else if let Some(rel) = caps.get(2) {
                        m3u8::resolve_url(rel.as_str(), &base_url)
                    } else {
                        continue;
                    };
                    links.insert(full_url);
                }
            }
        }

        // 按文件名去重
        let mut seen: HashSet<String> = HashSet::new();
        let mut result: HashSet<String> = HashSet::new();
        for link in links {
            if let Some(name) = Path::new(&link).file_name().map(|n| n.to_string_lossy().to_string()) {
                if seen.insert(name) {
                    result.insert(link);
                }
            } else {
                result.insert(link);
            }
        }
        result
    }

    // ============================================================
    // one_time_download — 一次性全量下载（小文件用）
    // ============================================================
    pub async fn one_time_download(&self, url: &str, file_name: &str) {
        log::info!("选择一次性下载方式");
        match self.client.get(url).await {
            Ok(resp) => {
                match resp.bytes().await {
                    Ok(data) => {
                        if let Err(e) = std::fs::write(file_name, &data) {
                            log::error!("写入文件 {} 失败: {}", file_name, e);
                        }
                    }
                    Err(e) => log::error!("下载文件 {} 出错: {}", url, e),
                }
            }
            Err(e) => log::error!("下载文件 {} 出错: {}", url, e),
        }
    }

    // ============================================================
    // stream_download — 流式下载（中等文件用）
    // ============================================================
    pub async fn stream_download(&self, url: &str, file_name: &str) {
        let client = self.client.inner();
        match client.get(url).send().await {
            Ok(resp) => {
                let total_size = resp
                    .headers()
                    .get("Content-Length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);

                let pb = ProgressBar::new(total_size);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}) {msg}")
                        .unwrap()
                        .progress_chars("#>-"),
                );
                pb.set_message(file_name.to_string());

                let mut file = match tokio::fs::File::create(file_name).await {
                    Ok(f) => f,
                    Err(e) => {
                        log::error!("创建文件 {} 失败: {}", file_name, e);
                        return;
                    }
                };

                let mut stream = resp.bytes_stream();
                while let Some(chunk_result) = futures::StreamExt::next(&mut stream).await {
                    match chunk_result {
                        Ok(chunk) => {
                            if let Err(e) = file.write_all(&chunk).await {
                                log::error!("写入文件 {} 失败: {}", file_name, e);
                                break;
                            }
                            pb.inc(chunk.len() as u64);
                        }
                        Err(e) => {
                            log::error!("流式下载 {} 出错: {}", file_name, e);
                            break;
                        }
                    }
                }

                pb.finish_and_clear();
                log::info!("文件 {} 下载完成，保存路径: {}", url, file_name);
            }
            Err(e) => log::error!("流式下载文件 {} 时发生错误: {}", file_name, e),
        }
    }

    // ============================================================
    // multi_thread_stream_download — 多线程分片下载（大文件用）
    // ============================================================
    pub async fn multi_thread_stream_download(&self, url: &str, file_name: &str) {
        let client = self.client.inner();

        // 获取文件大小
        let (file_size, supports_range) = match client.head(url).send().await {
            Ok(resp) => {
                let size = resp
                    .headers()
                    .get("Content-Length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
                let range = resp
                    .headers()
                    .get("Accept-Ranges")
                    .map(|v| v.to_str().unwrap_or(""))
                    .unwrap_or("");
                (size, range.contains("bytes"))
            }
            Err(e) => {
                log::error!("获取文件信息失败: {}", e);
                return;
            }
        };

        if file_size == 0 || !supports_range {
            log::warn!("服务器不支持范围请求，将使用单线程下载");
            self.stream_download(url, file_name).await;
            return;
        }

        // 预分配文件
        if let Err(e) = std::fs::File::create(file_name)
            .and_then(|f| f.set_len(file_size))
        {
            log::error!("预分配文件大小失败: {}", e);
            return;
        }

        // 计算分片范围
        let part_size = file_size / self.threads as u64;
        let mut ranges: Vec<(u64, u64)> = Vec::new();
        for i in 0..self.threads {
            let start = i as u64 * part_size;
            let end = if i < self.threads - 1 {
                start + part_size - 1
            } else {
                file_size - 1
            };
            ranges.push((start, end));
        }

        let pb = Arc::new(Mutex::new(ProgressBar::new(file_size)));
        {
            let p = pb.lock().await;
            p.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}) {msg}")
                    .unwrap()
                    .progress_chars("#>-"),
            );
            p.set_message(file_name.to_string());
        }

        let url = Arc::new(url.to_string());
        let file_name = Arc::new(file_name.to_string());
        let mut handles = Vec::new();

        for (start, end) in &ranges {
            let start = *start;
            let end = *end;
            let url = url.clone();
            let file_name = file_name.clone();
            let pb = pb.clone();
            let client = client.clone();

            handles.push(tokio::spawn(async move {
                let headers = {
                    let mut h = HeaderMap::new();
                    h.insert("Range", format!("bytes={}-{}", start, end).parse().unwrap());
                    h
                };

                match client.get(url.as_str()).headers(headers).send().await {
                    Ok(resp) => {
                        match resp.bytes().await {
                            Ok(data) => {
                                // 写入文件对应位置
                                let mut file = match std::fs::OpenOptions::new()
                                    .write(true)
                                    .open(file_name.as_str())
                                {
                                    Ok(f) => f,
                                    Err(e) => {
                                        log::error!("打开文件失败: {}", e);
                                        return;
                                    }
                                };
                                use std::io::Seek;
                                if let Err(e) = file.seek(std::io::SeekFrom::Start(start)) {
                                    log::error!("文件定位失败: {}", e);
                                    return;
                                }
                                if let Err(e) = file.write_all(&data) {
                                    log::error!("写入文件失败: {}", e);
                                    return;
                                }
                                let p = pb.lock().await;
                                p.inc(data.len() as u64);
                            }
                            Err(e) => {
                                log::error!("下载分片 {}-{} 失败: {}", start, end, e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("下载分片 {}-{} 失败: {}", start, end, e);
                    }
                }
            }));
        }

        for handle in handles {
            let _ = handle.await;
        }

        pb.lock().await.finish_and_clear();
        log::info!("文件 {} 下载完成，保存路径: {}", url, file_name);
    }

    // ============================================================
    // file_download — 根据文件大小选择下载策略
    // ============================================================
    pub async fn file_download(&self, file_info: &FileInfo) {
        let url = &file_info.url;
        let file_name = file_info.file_name.as_deref().unwrap_or("download");
        let file_size = file_info.file_size;

        if file_size < 15 * 1024 * 1024 {
            self.one_time_download(url, file_name).await;
        } else if file_size < 30 * 1024 * 1024 {
            self.stream_download(url, file_name).await;
        } else {
            self.multi_thread_stream_download(url, file_name).await;
        }
    }

    // ============================================================
    // m3u8_download — 下载 M3U8 视频流
    // ============================================================
    pub async fn m3u8_download(&self, file_info: &FileInfo) {
        let file_url = &file_info.url;
        let file_name = file_info.file_name.as_deref().unwrap_or("video");

        let mut save_path = {
            let stripped = file_name
                .strip_suffix(".m3u8")
                .unwrap_or(file_name);
            format!("{}.ts", stripped)
        };

        if Path::new(&save_path).exists() {
            let now = chrono::Local::now();
            save_path = format!(
                "{}{}.ts",
                file_name.strip_suffix(".m3u8").unwrap_or(file_name),
                now.format("%H%M%S")
            );
        }

        // 递归解析 M3U8（处理 variant playlist）
        let parse_result = self.resolve_m3u8(file_url).await;
        let segments = parse_result.segments.clone();
        let key_method = parse_result.key_method.clone();
        let key_uri = parse_result.key_uri.clone();
        let key_iv = parse_result.key_iv.clone();

        if segments.is_empty() {
            log::info!("m3u8 中未找到有效的 TS 片段");
            return;
        }

        let total = segments.len();
        let pb = Arc::new(Mutex::new(ProgressBar::new(total as u64)));
        {
            let p = pb.lock().await;
            p.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} TS {msg}")
                    .unwrap()
                    .progress_chars("#>-"),
            );
            p.set_message("M3U8 下载进度");
        }

        let (tx, mut rx) = mpsc::channel::<(usize, Option<Vec<u8>>)>(total * 2);

        // 下载线程
        let key_method = key_method.clone();
        let key_uri = key_uri.clone();
        let key_iv = key_iv.clone();
        let client = self.client.inner().clone();
        let out_dir = self.out_dir.clone();

        let download_handle = {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut tasks = Vec::new();
                for (index, segment) in segments.iter().enumerate() {
                    let segment = segment.clone();
                    let tx = tx.clone();
                    let key_method = key_method.clone();
                    let key_uri = key_uri.clone();
                    let key_iv = key_iv.clone();
                    let client = client.clone();
                    let out_dir = out_dir.clone();

                    tasks.push(tokio::spawn(async move {
                        let data = ts_download_de(
                            &client,
                            &out_dir,
                            &segment,
                            key_method.as_deref(),
                            key_uri.as_deref(),
                            key_iv.as_deref(),
                        )
                        .await;
                        let _ = tx.send((index, data)).await;
                    }));
                }
                for task in tasks {
                    let _ = task.await;
                }
            })
        };

        // 写入线程 —— 按序号顺序写入
        let save_path_clone = save_path.clone();
        let pb_clone = pb.clone();
        let writer_handle = tokio::spawn(async move {
            let mut file = match tokio::fs::File::create(&save_path_clone).await {
                Ok(f) => f,
                Err(e) => {
                    log::error!("创建文件 {} 失败: {}", save_path_clone, e);
                    return;
                }
            };

            let mut expected_index = 0;
            let mut pending: HashMap<usize, Option<Vec<u8>>> = HashMap::new();

            while expected_index < total {
                match rx.recv().await {
                    Some((index, data)) => {
                        if index == expected_index {
                            match data {
                                Some(bytes) => {
                                    if let Err(e) = file.write_all(&bytes).await {
                                        log::error!("写入 TS 片段 {} 失败: {}", expected_index, e);
                                        break;
                                    }
                                }
                                None => {
                                    log::error!("TS 片段 {} 下载失败，终止合并", expected_index);
                                    break;
                                }
                            }
                            expected_index += 1;
                            {
                                let p = pb_clone.lock().await;
                                p.inc(1);
                            }

                            // 处理缓存中已到达的后继片段
                            while let Some(data) = pending.remove(&expected_index) {
                                match data {
                                    Some(bytes) => {
                                        if let Err(e) = file.write_all(&bytes).await {
                                            log::error!("写入 TS 片段 {} 失败: {}", expected_index, e);
                                            break;
                                        }
                                    }
                                    None => {
                                        log::error!("TS 片段 {} 下载失败，终止合并", expected_index);
                                        return;
                                    }
                                }
                                expected_index += 1;
                                {
                                    let p = pb_clone.lock().await;
                                    p.inc(1);
                                }
                            }
                        } else if index > expected_index {
                            pending.insert(index, data);
                        }
                        // index < expected_index 的情况忽略（已处理过）
                    }
                    None => break, // channel closed
                }
            }
        });

        // 等待全部完成
        let _ = download_handle.await;
        let _ = writer_handle.await;

        pb.lock().await.finish_and_clear();
        log::info!("m3u8 视频下载完成，保存路径: {}", save_path);
    }

    /// 递归解析 M3U8（处理 variant playlist）
    async fn resolve_m3u8(&self, url: &str) -> M3u8ParseResult {
        let resp = match self.client.get(url).await {
            Ok(r) => r,
            Err(e) => {
                log::error!("解析 M3U8 文件 {} 出错: {}", url, e);
                return M3u8ParseResult::empty();
            }
        };

        let content = match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                log::error!("读取 M3U8 文件 {} 出错: {}", url, e);
                return M3u8ParseResult::empty();
            }
        };

        let result = m3u8::parse_m3u8(&content, url);

        // 如果是 variant playlist，递归解析最高画质
        if result.key_method.as_deref() == Some("__STREAM_INF__") && !result.segments.is_empty() {
            let next_url = &result.segments[0];
            return Box::pin(self.resolve_m3u8(next_url)).await;
        }

        result
    }

    // ============================================================
    // download — 入口分发
    // ============================================================
    pub async fn download(&self, url: &str) {
        let file_info = match self.get_file_info(url).await {
            Some(info) => info,
            None => {
                log::error!("无法获取文件信息: {}", url);
                return;
            }
        };
        log::info!("获取文件信息: url={}, type={:?}, size={}",
            file_info.url, file_info.file_type, file_info.file_size);

        match file_info.file_type.as_deref() {
            Some(".html") => {
                let links = self.html_parse(url).await;
                if links.is_empty() {
                    log::info!("未从 {} 获取到媒体文件", url);
                }
                for link in links {
                    let downloader = self; // 借用自身
                    Box::pin(downloader.download(&link)).await;
                }
            }
            Some(".m3u8") => {
                self.m3u8_download(&file_info).await;
            }
            _ => {
                self.file_download(&file_info).await;
            }
        }
    }
}

// ============================================================
// ts_download_de — 下载单个 TS 片段（含 AES-128-CBC 解密）
// ============================================================
async fn ts_download_de(
    client: &reqwest::Client,
    _out_dir: &Path,
    segment: &str,
    key_method: Option<&str>,
    key_uri: Option<&str>,
    key_iv: Option<&str>,
) -> Option<Vec<u8>> {
    const MAX_RETRIES: u32 = 2;

    // 获取加密密钥
    let mut key: Option<Vec<u8>> = None;
    let mut iv_bytes: Option<[u8; 16]> = None;

    if let (Some("AES-128"), Some(uri)) = (key_method, key_uri) {
        match client.get(uri).send().await {
            Ok(resp) => {
                if let Ok(data) = resp.bytes().await {
                    key = Some(data.to_vec());
                    if let Some(iv_str) = key_iv {
                        let iv_hex = iv_str.strip_prefix("0x").unwrap_or(iv_str);
                        if let Ok(iv) = hex::decode(iv_hex) {
                            if iv.len() == 16 {
                                let mut arr = [0u8; 16];
                                arr.copy_from_slice(&iv);
                                iv_bytes = Some(arr);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("获取加密密钥失败: {}", e);
                return None;
            }
        }
    }

    // 重试下载
    for retry in 0..=MAX_RETRIES {
        match client.get(segment).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if status != 200 {
                    log::error!("下载 TS 文件 {} HTTP {} ", segment, status);
                    // 进入重试
                } else {
                    // 流式下载 + 分块解密（对应 Python iter_content 方式）
                    let result = stream_download_and_decrypt(
                        resp, &key, &iv_bytes).await;
                    match result {
                        Ok(decrypted) => {
                            if decrypted.is_empty() || decrypted[0] != 0x47 {
                                log::warn!("{} 不是有效的 TS 数据流", segment);
                                return None;
                            }
                            return Some(decrypted);
                        }
                        Err(e) => {
                            log::error!("下载 TS 文件 {} 时发生错误: {}", segment, e);
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("下载 TS 文件 {} 时发生错误: {}", segment, e);
            }
        }

        if retry < MAX_RETRIES {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    log::error!("下载 {} 失败，已达到最大重试次数 {}", segment, MAX_RETRIES);
    None
}

// ============================================================
// AES-128-CBC 状态化解密器（支持分块流式解密）
// ============================================================
struct CbcDecryptor {
    cipher: Aes128,
    prev_ct: [u8; 16],
}

impl CbcDecryptor {
    fn new(key: &[u8; 16], iv: &[u8; 16]) -> Self {
        let cipher = Aes128::new(GenericArray::from_slice(key));
        CbcDecryptor {
            cipher,
            prev_ct: *iv,
        }
    }

    /// 解密一块数据（长度必须是 16 的倍数），保持 CBC 状态跨调用
    fn decrypt(&mut self, data: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(data.len());
        for chunk in data.chunks(16) {
            let mut block = GenericArray::clone_from_slice(chunk);
            self.cipher.decrypt_block(&mut block);
            for (b, p) in block.iter_mut().zip(self.prev_ct.iter()) {
                *b ^= p;
            }
            result.extend_from_slice(&block);
            self.prev_ct.copy_from_slice(chunk);
        }
        result
    }
}

// ============================================================
// 流式下载 + 分块解密
// ============================================================
async fn stream_download_and_decrypt(
    resp: reqwest::Response,
    key: &Option<Vec<u8>>,
    iv_bytes: &Option<[u8; 16]>,
) -> Result<Vec<u8>, reqwest::Error> {
    use futures::StreamExt;

    let mut stream = resp.bytes_stream();
    let mut result = Vec::new();

    if let Some(k) = key {
        // 加密流：状态化分块解密
        let iv = iv_bytes.unwrap_or([0u8; 16]);
        let mut decryptor = CbcDecryptor::new(
            k.as_slice().try_into().unwrap(),
            &iv,
        );
        let mut buf = Vec::new(); // 缓存不足 16 字节的尾部
        let mut first = true;

        while let Some(chunk) = stream.next().await {
            let data = chunk?;
            buf.extend_from_slice(&data);

            let aligned = (buf.len() / 16) * 16;
            if aligned == 0 {
                continue;
            }

            let decrypted = decryptor.decrypt(&buf[..aligned]);
            if first {
                if decrypted.is_empty() || decrypted[0] != 0x47 {
                    // 解密后第一字节不是 TS 同步头 → 解密失败
                    return Ok(Vec::new()); // 由调用者处理
                }
                first = false;
            }
            result.extend_from_slice(&decrypted);
            buf.drain(..aligned);
        }
    } else {
        // 无加密：直接流式收集
        let mut first = true;
        while let Some(chunk) = stream.next().await {
            let data = chunk?;
            if first {
                if data.is_empty() || data[0] != 0x47 {
                    return Ok(Vec::new());
                }
                first = false;
            }
            result.extend_from_slice(&data);
        }
    }

    Ok(result)
}
