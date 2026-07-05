mod converter;
mod downloader;
mod http;
mod m3u8;

use clap::{ArgGroup, Parser};
use downloader::Downloader;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "mu8", about = "文件下载器 - 支持 M3U8/TS 流媒体下载、多线程分片下载、TS 转 MP4")]
#[command(group = ArgGroup::new("action").required(true).multiple(false))]
struct Cli {
    /// 要下载文件的 URL 或 HTML 页面 URL
    #[arg(long, group = "action")]
    url: Option<String>,

    /// 将 ts 文件转码为 MP4，必须包含 ts 文件路径
    #[arg(long, group = "action", value_name = "TS_PATH")]
    to4: Option<String>,

    /// mp4 文件保存路径（与 --to4 配合使用）
    #[arg(long, value_name = "MP4_PATH")]
    mp4_path: Option<String>,

    /// 保存文件的目录，默认 temp_download
    #[arg(long, default_value = "./temp_download")]
    dir: String,

    /// 线程数，默认是 CPU 核心数的一半
    #[arg(long)]
    threads: Option<usize>,

    /// 分片大小，单位为字节，默认 500KB
    #[arg(long, default_value_t = 500 * 1024)]
    chunk_size: usize,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let start = Instant::now();
    let cli = Cli::parse();

    if let Some(ts_path) = cli.to4 {
        // TS 转 MP4 模式
        let mp4_path = cli
            .mp4_path
            .unwrap_or_else(|| format!("{}.mp4", ts_path));
        converter::ts_to_mp4(&ts_path, &mp4_path)?;
    } else if let Some(url) = cli.url {
        // 下载模式
        let downloader = Downloader::new(
            Some(&cli.dir),
            cli.threads,
            Some(cli.chunk_size),
            false,
        )?;
        downloader.download(&url).await;
    }

    log::info!("程序运行时间: {:.2} 秒", start.elapsed().as_secs_f64());
    Ok(())
}
