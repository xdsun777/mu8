use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use std::io::BufRead;
use std::process::{Command, Stdio};

/// 将本地 TS 文件转码为 MP4
/// 对应 Python: Downloader.ts_to_mp4()
pub fn ts_to_mp4(
    ts_path: &str,
    mp4_path: &str,
) -> Result<bool, anyhow::Error> {
    // 1. 获取 TS 文件时长（用于进度条）
    let total_seconds = get_duration(ts_path).unwrap_or(0);

    // 2. 初始化进度条
    let pb = ProgressBar::new(total_seconds);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}s/{len:7}s {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.set_message("TS 转 MP4");

    // 3. 构建 ffmpeg 命令
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-i", ts_path,
        "-c:v", "libx264",
        "-preset", "fast",
        "-crf", "23",
        "-c:a", "aac",
        "-b:a", "128k",
        "-movflags", "+faststart",
        "-progress", "pipe:1",
        "-nostats",
        "-y",
        mp4_path,
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!("未找到 ffmpeg 可执行文件，请检查 ffmpeg 是否正确安装并配置了环境变量")
        } else {
            anyhow::anyhow!("启动 ffmpeg 失败: {}", e)
        }
    })?;

    // 4. 监控转换进度
    let stdout = child.stdout.take().unwrap();
    let reader = std::io::BufReader::new(stdout);
    let time_re = Regex::new(r"out_time_us=(\d+)").unwrap();
    let mut current_time: u64 = 0;

    for line in reader.lines() {
        if let Ok(line) = line {
            if let Some(caps) = time_re.captures(&line) {
                if let Ok(us) = caps[1].parse::<u64>() {
                    let new_seconds = us / 1_000_000;
                    if new_seconds > current_time {
                        pb.set_position(new_seconds);
                        current_time = new_seconds;
                    }
                }
            }
        }
    }

    pb.finish_and_clear();

    // 5. 检查返回码
    let status = child.wait()?;
    if status.success() {
        log::info!("TS 文件 {} 成功转换为 MP4 文件 {}", ts_path, mp4_path);
        Ok(true)
    } else {
        log::error!("ffmpeg 转换失败，退出码: {:?}", status.code());
        Ok(false)
    }
}

/// 获取 TS 文件时长（秒）
fn get_duration(ts_path: &str) -> Result<u64, anyhow::Error> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            ts_path,
        ])
        .output()?;

    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout);
        if let Ok(secs) = s.trim().parse::<f64>() {
            return Ok(secs.ceil() as u64);
        }
    }
    Ok(0)
}
