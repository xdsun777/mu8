# 文件下载器

## 功能说明

- 支持一次性下载
- 支持流式下载
- 支持多线程流式下载
- 支持进度条显示
- 支持自定义下载目录
- 支持ts无损转mp4
- 支持自定义文件名 x 未实现
- 支持断点续传 x 未实现
- 支持多线程分片下载 x 未实现



## 安装依赖

```powsershell
winget install "Gyan.FFmpeg"
pip install -r requirements.txt
```

## 使用说明

- --to4 `ts_path`         将ts文件转码为MP4，必须包含ts文件路径，输出文件为*.mp4
- --mp4_path `MP4_PATH`   mp4文件保存路径,默认当前目录,同ts文件名
- --url `url`             要下载文件的 URL 或 HTML 页面 URL
- --dir `DIR`             保存文件的目录，默认temp_download
- --threads `THREADS`     线程数，默认是 CPU 核心数的一半
- --chunk-size `CHUNK_SIZE` 分片大小，单位为字节，默认 500KB

## 示例

### 下载文件
```bash
python main.py --url 'http://127.0.0.1:8000/v2rayN-linux-64.deb' --dir ./m3u8_downloads --threads 4 --chunk-size 1024*1024
```

### ts文件转码mp4
```bash
python main.py --to4 ./m3u8_downloads/test.ts --mp4_path ./m3u8_downloads/test.mp4
```
