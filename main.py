import io
import subprocess
import threading
from queue import Queue
from typing import Dict, List, Optional
from urllib.parse import urljoin
import concurrent.futures
import sys
import os
import time
import argparse
import math
import re
from Crypto.Cipher import AES
import psutil
from bs4 import BeautifulSoup
import requests
from tqdm import tqdm

from axios import Request as axios
from axios import AxiosError as axiosError

import logging
# 配置日志
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s',
    handlers=[
        logging.StreamHandler()
    ]
)

# 初始化 axios
axios = axios()


class Downloader:
    response_type = {
        'application/vnd.apple.mpegurl':'.m3u8',
        'audio/mpegurl':'.m3u8',
        'text/css':'.css',
        'text/javascript':'.js',
        'text/csv':'.csv',
        'text/calendar':'.ics',
        'text/html':'.html' ,
        'application/json':'.json' ,
        'application/xml':'.xml' ,
        'text/plain': '.text',
        'application/pdf': '.pdf',
        'application/msword': '.doc',
        'application/vnd.openxmlformats-officedocument.wordprocessingml.document': '.docx',
        'application/vnd.ms-excel': '.xls',
        'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet':'.xlsx',
        'application/vnd.ms-powerpoint': '.ppt',
        'application/vnd.openxmlformats-officedocument.presentationml.presentation': '.pptx',
        'application/zip': '.zip',
        'application/x-rar-compressed': '.rar',
        'application/x-7z-compressed': '.7z',
        'application/x-tar': '.tar',
        'application/gzip': '.gz',
        'application/x-bzip2': '.bzip2',
        'audio/mpeg': '.mp3',
        'video/mp4': '.mp4',
        'video/x-msvideo': '.avi',
        'video/x-flv': '.flv',
        'video/quicktime': '.mov',
        'video/x-ms-wmv': '.wmv',
        'image/jpeg': '.jpeg',
        'image/png': '.png',
        'image/gif': '.gif',
        'image/svg+xml': '.svg',
        'image/webp': '.webp',
        'image/x-icon': '.ico',
        'application/octet-stream': '.exe',
        'application/x-msdownload': '.msi',
        'application/vnd.android.package-archive': '.apk',
        'audio/wav':'.wav',
        'video/mpeg': '.mpeg',
        'application/x-mpegurl':'.m3u8',
        'audio/ogg': '.ogg',
        'video/mp2t': '.ts'
    }

    def __init__(self,out_dir=None,threads: int=None, chunk_size: int=None,goon: bool=False):
        self.out_dir = os.path.abspath(out_dir) if out_dir else os.path.abspath('./temp_download')
        self.threads = min(threads,os.cpu_count()//2,16)
        self.chunk_size = chunk_size if chunk_size else 1024*1024*500
        self.goon = False # 续点继传控制参数

        self.lock = threading.Lock()

        os.makedirs(self.out_dir, exist_ok=True)

    def read_url_header_content(self, url: str) -> str:
        """
        读取 URL 的头部内容。
        功能：
            1. 发送 HTTP GET 请求获取 URL 的头部内容。
            2. 处理获取头部内容的错误和异常。
        :param url:
        :param response:
        :return: m3u8 或 html 或 None
        """
        try:
            response = axios.get(url)
            first_chunk = next(response.iter_content(chunk_size=1024), b'')
            if first_chunk.startswith(b'#EXTM3U'):
                return '.m3u8'
            elif first_chunk.startswith(b'<!DOCTYPE html>') or first_chunk.startswith(b'<!doctype html>'):
                return '.html'
            elif first_chunk.startswith(b'\xFF\xD8\xFF'):  # JPEG 文件头
                return '.jpeg'
            elif first_chunk.startswith(b'\x89PNG\r\n\x1a\n'):  # PNG 文件头
                return '.png'
            elif first_chunk.startswith(b'GIF87a') or first_chunk.startswith(b'GIF89a'):  # GIF 文件头
                return '.gif'
            elif first_chunk.startswith(b'\x00\x00\x00\x14ftypisom'):  # MP4
                return '.mp4'
            elif first_chunk.startswith(b'RIFF'):  # AVI
                return '.avi'
            elif first_chunk.startswith(b'FLV\x01'):  # FLV
                return '.flv'
            elif first_chunk.startswith(b'\x00\x00\x00\x14ftypqt  '):  # MOV
                return '.mov'
            elif first_chunk.startswith(b'0&\xb2u\x8e\x66\xcf\x11\xa6\xd9\x00\xaa\x00b\xce\x6c'):  # WMV
                return '.wmv'
            elif first_chunk.startswith(b'\x00\x00\x01\xb3'):  # MPEG
                return '.mpeg'
            elif first_chunk.startswith(b'\x00\x00\x00\x20ftypM4A '):  # M4A
                return '.m4a'
            elif first_chunk.startswith(b'ID3'):  # MP3
                return '.mp3'
            elif first_chunk.startswith(b'PK\x03\x04'):  # ZIP
                return '.zip'
            elif first_chunk.startswith(b'Rar!\x1a\x07\x00'):  # RAR
                return '.rar'
            elif first_chunk.startswith(b'7z\xbc\xaf\x27\x1c'):  # 7z
                return '.7z'
            elif first_chunk.startswith(b'\x1f\x8b\x08'):  # GZIP
                return '.gz'
            elif first_chunk.startswith(b'\x42\x5a\x68'):  # BZIP2
                return '.bz2'
            elif first_chunk.startswith(b'\x00\x00\x00\x14ftypM4S '):  # M4S
                return '.m4s'
            elif first_chunk.startswith(b'PK\x05\x06'):  # APK
                return '.apk'
            elif first_chunk.startswith(b'PK\x03\x04'):  # APK
                return '.apk'
            elif first_chunk.startswith(b'MZP'):
                return '.exe'
            else:
                # logging.info(f"判断文件真实类型: {first_chunk}")
                pass
        except axiosError as e:
            logging.error(f"直接判断url指向服务器文件的真实类型: {e}")
        return None

    def get_file_info(self,url:str) -> Optional[int]:
        """
        获取文件信息。
        功能：
            1. 发送 HTTP HEAD 请求获取文件大小。
            2. 处理获取文件大小的错误和异常。
        :param url:
        :return: {
            url:
            file_name:
            file_type:
            file_size:
        }
        """
        file_info = {
            'url': url,
            'file_name': None,
            'file_type': None,
            'file_size': 0
        }
        try:
            response = axios.head(url)
            if response.status_code == 200:
                content_length = response.headers.get('Content-Length')  # 以字节为单位
                if content_length:
                    file_info['file_size'] = int(content_length)
                content_type = response.headers.get('Content-Type')
                if content_type:
                    file_info['file_type'] = self.response_type.get(content_type.split(';')[0].lower())
                url_content_type = self.read_url_header_content(url)
                if url_content_type:
                    file_info['file_type'] = url_content_type
                f_name = re.sub(r'[\\/*?:"<>|]', '', os.path.basename(url)).strip()
                # f_name存在且!=''
                if f_name and f_name !='':
                    if f_name.endswith(file_info['file_type']):
                        file_info['file_name'] = f_name
                    else:
                        file_info['file_name'] = f_name + url_content_type
                else:# f_name不存在或=''
                    f_name = re.sub(r'[\\/*?:"<>|]', '', re.split('/', url)[2]) + time.strftime("H%M%S")+file_info['file_type']
                    file_info['file_name'] = f_name
                if os.path.exists(file_info['file_name']) and self.goon is False:
                    file_info['file_name'] = time.strftime("%M%S") + file_info['file_name']

                file_info['file_name'] = os.path.join(self.out_dir, file_info['file_name'])
                if os.path.exists(file_info['file_name']):
                    file_info['file_name'] = time.strftime("%H%M%S") + file_info['file_name']
                return file_info
            else:
                logging.info(f"获取文件信息失败: {response.status_code}")
                sys.exit(1)
        except axiosError as e:
            logging.error(f"获取文件信息失败: {e}")
            sys.exit(1)
        return file_info

    def html_parse(self,url:str):
        """
        解析 HTML 页面，提取所有媒体文件链接。
        功能：
            1. 发送 HTTP GET 请求获取 HTML 页面内容。
            2. 解析 HTML 页面，提取所有文件链接。
            3. 处理解析过程中的错误和异常。
        :param url:
        :return: set(str)
        """
        try:
            response = axios.get(url)
            # 使用 chardet 检测编码
            # detected_encoding = chardet.detect(response.content)['encoding']
            # if detected_encoding:
            #     response.encoding = detected_encoding
            response.encoding = response.apparent_encoding
            with open('./test.html','wb') as f:
                f.write(response.content)
            soup = BeautifulSoup(response.text, 'html.parser')
            base_url = url.rsplit('/', 1)[0] + '/'
            links = set()
            # 定义常见包含链接的属性
            link_attributes = ['href', 'src', 'data-src', 'data-url', 'data-file', 'data-config', 'data-stream',
                               'data-m3u8']
            # 遍历所有标签
            for tag in soup.find_all(True):
                for attr in link_attributes:
                    value = tag.get(attr)
                    if value is not None:
                        value = value.replace('\\', '')
                        pattern = re.compile(r'https?://[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/[^?]+\.m3u8(\?[a-zA-Z0-9_=&-]+)?')
                        match = pattern.search(value)
                        if match:  # 添加判断，确保 match 不为 None
                            link = match.group(0)
                            links.add(link)

            # 从 JavaScript 脚本中提取链接
            script_tags = soup.find_all(True)
            for script_tag in script_tags:
                script_content = script_tag.string
                if script_content:
                    # 使用正则表达式匹配绝对路径和相对路径的链接
                    pattern = re.compile(r'(https?://[^\s"\']+\.(?:mp4|mp3|m3u8))|([^\s"\']+\.(?:mp4|mp3|m3u8))',
                                         re.IGNORECASE)
                    matches = pattern.finditer(script_content)
                    for match in matches:
                        full_url = match.group(1) or urljoin(base_url, match.group(2))
                        links.add(full_url)
        except axiosError as e:
            logging.error(f"解析 HTML 页面: {url} 出错: {e}")
        # 去除重复的 URL 链接 list(dict.fromkeys(links))
        set_links = set()
        added_endings = set()  # 用于记录已经添加过的文件名
        for link in links:
            end = os.path.basename(link)
            if end not in added_endings:
                set_links.add(link)
                added_endings.add(end)
        return set_links

    def one_time_download(self,url: str,file_name:str):
        """
        下载单个文件。
        功能：
            1. 发送 HTTP GET 请求下载文件。
            2. 处理下载过程中的错误和异常。
            3. 显示下载进度。
        :param url:
        :param file_name:
        :return:
        """
        logging.info("选择一次性下载方式")
        try:
            response = axios.get(url)
            with open(file_name, 'wb') as f:
                f.write(response.content)
        except axiosError as e:
            logging.error(f"下载文件: {url} 出错: {e}")

    def stream_download(self,url:str,file_name:str):
        """
        流式下载文件。
        功能：
            1. 发送 HTTP GET 请求下载文件。
            2. 处理下载过程中的错误和异常。
            3. 显示下载进度。
        :param url:
        :param file_name:
        :return:
        """
        # logging.info("选择流式下载")
        try:
            # 发送 GET 请求，开启流式下载
            with requests.get(url, stream=True) as response:
                # 获取文件总大小，若响应头中无 Content-Length 字段则默认 0
                total_size = int(response.headers.get('Content-Length', 0))
                # 初始化进度条
                progress_bar = tqdm(
                    total=total_size,
                    unit='B',
                    unit_scale=True,
                    unit_divisor=1024,
                    desc=url,
                    dynamic_ncols=True
                )

                # 以二进制追加模式打开文件
                with open(file_name, 'wb') as f:
                    # 迭代响应内容，每次读取指定大小的块
                    for chunk in response.iter_content(chunk_size=self.chunk_size):
                        if chunk:
                            # 将块写入文件
                            f.write(chunk)
                            # 更新进度条
                            progress_bar.update(len(chunk))
                # 关闭进度条
                progress_bar.close()
                logging.info(f"文件 {file_name} 下载完成，保存路径: {file_name}")
        except axiosError as e:
            logging.error(f"流式下载文件 {file_name} 时发生未知错误: {e}")

    def multi_thread_stream_download(self,url:str,file_name:str):
        """
        多线程流式下载文件。
        功能：
            1. 发送 HTTP GET 请求下载文件。
            2. 处理下载过程中的错误和异常。
            3. 显示下载进度。
        :param url:
        :param file_name:
        :return:
        """
        # logging.info("选择多线程流式下载")
        try:
            # 发送 HEAD 请求获取文件信息
            head_response = requests.head(url)
            head_response.raise_for_status()
            file_size = int(head_response.headers.get('Content-Length', 0))
            if 'bytes' not in head_response.headers.get('Accept-Ranges', ''):
                logging.warning("服务器不支持范围请求，将使用单线程下载")
                self.stream_download(url, file_name)
                return

            # 创建一个空文件，预分配空间
            with open(file_name, 'wb') as f:
                f.truncate(file_size)

            # 计算每个线程下载的字节范围
            part_size = file_size // self.threads
            ranges = []
            for i in range(self.threads):
                start = i * part_size
                end = start + part_size - 1 if i < self.threads - 1 else file_size - 1
                ranges.append((start, end))

            # 使用线程池进行多线程下载
            with concurrent.futures.ThreadPoolExecutor(max_workers=self.threads) as executor:
                futures = []
                for index, (start, end) in enumerate(ranges):
                    future = executor.submit(self._download_chunk, url, start, end, file_name, index)
                    futures.append(future)

                # 初始化进度条
                progress_bar = tqdm(
                    total=file_size,
                    unit='B',
                    unit_scale=True,
                    unit_divisor=1024,
                    desc=file_name,
                    dynamic_ncols=True
                )

                # 监控下载进度
                completed_size = 0
                for future in concurrent.futures.as_completed(futures):
                    try:
                        future.result()
                        # 计算已完成的片段大小
                        start, end = ranges[futures.index(future)]
                        part_size = end - start + 1
                        completed_size += part_size
                        progress_bar.update(part_size)
                    except Exception as e:
                        logging.error(f"下载过程中出现错误: {e}")

                progress_bar.close()

            logging.info(f"文件 {url} 下载完成，保存路径: {file_name}")
        except axiosError as e:
            logging.error(f"下载文件 {url} 时发生错误: {e}")

    def _download_chunk(self, url, start, end, file_path, chunk_index):
        """
        下载文件的一个片段
        :param url: 文件的 URL
        :param start: 片段的起始字节位置
        :param end: 片段的结束字节位置
        :param file_path: 保存文件的路径
        :param chunk_index: 片段的索引，用于顺序写入
        """
        headers = {'Range': f'bytes={start}-{end}'}
        try:
            with requests.get(url, headers=headers, stream=True) as response:
                response.raise_for_status()
                data_chunks = []
                for chunk in response.iter_content(chunk_size=self.chunk_size):
                    if chunk:
                        data_chunks.append(chunk)

                # 加锁，确保按顺序写入
                with self.lock:
                    with open(file_path, 'r+b') as f:
                        f.seek(start)
                        for data in data_chunks:
                            f.write(data)
        except axiosError as e:
            logging.error(f"下载片段 {start}-{end} 失败: {e}")

    def file_download(self,file_info:dict):
        """
        通用下载。
        逻辑：
            1.如果文件大小小于 15Mb，使用one-time-download下载。
            2.如果文件大小大于 15Mb且小于30Mb，使用流式下载。
            3.如果文件大小大于30Mb，使用多线程流式下载。
        :param file_info:
        :return:
        """
        # logging.info(f"调用通用下载:{file_info}")
        url,file_name,file_type,file_size = file_info.get('url'),file_info.get('file_name'),file_info.get('file_type'),file_info.get('file_size')
        if file_size < 15 * 1024 * 1024:
            self.one_time_download(url,file_name)
        elif file_size < 30 * 1024 * 1024:
            self.stream_download(url,file_name)
        else:
            self.multi_thread_stream_download(url,file_name)


    def m3u8_download(self,file_info:dict):
        """
        下载 m3u8 文件。
        功能：
            1. 解析 m3u8 文件，获取所有分片的 URL。
            2. 下载所有分片文件。
            3. 合并所有分片文件为一个完整的文件。
        :param file_info: dict {}
        :return:
        """
        # logging.info(f"调用下载 m3u8 文件:{file_info}")
        file_url,file_name,file_type = file_info.get('url'),file_info.get('file_name'),file_info.get('file_type')
        save_path = file_name+'.ts'
        if os.path.exists(save_path):
            save_path = os.path.splitext(save_path)[0]+ time.strftime("%H%M%S") +'.ts'
        ts_list_file = os.path.join(self.out_dir,'ts_list.txt')
        ts_list = None
        if os.path.exists(ts_list_file):
            os.remove(ts_list_file)
        parse_result = self.m3u8_parse(file_url)

        segments = parse_result["segments"]
        key_method = parse_result["key_method"]
        key_uri = parse_result["key_uri"]
        key_iv = parse_result["key_iv"]
        if not segments:
            logging.info("m3u8中未找到有效的 TS 片段。")
            return

        ts_list = [os.path.join(self.out_dir, segment.split('/')[-1]) for segment in segments]
        # 下载所有分片文件
        # 初始化队列，用于按顺序存储下载好的 TS 片段数据
        ts_queue = Queue()
        # 线程锁，用于确保写入文件时的线程安全
        write_lock = threading.Lock()
        # 初始化进度条
        progress_bar = tqdm(total=len(segments), unit='TS', desc="M3U8 下载进度")

        def download_and_enqueue(index, segment):
            """
            下载 TS 片段并将其加入队列
            :param index: 片段的索引
            :param segment: 片段的 URL
            """
            ts_data = self.ts_download_de(segment, key_method, key_uri, key_iv)
            if ts_data:
                ts_queue.put((index, ts_data))

        def write_ts_data():
            """按顺序从队列中取出 TS 片段并写入文件"""
            expected_index = 0
            with open(save_path, 'wb') as outfile:
                while True:
                    if not ts_queue.empty():
                        index, ts_data = ts_queue.get()
                        if index == expected_index:
                            with write_lock:
                                outfile.write(ts_data)
                            expected_index += 1
                            progress_bar.update(1)
                            if expected_index == len(segments):
                                break
                        else:
                            ts_queue.put((index, ts_data))

        # 启动写入线程
        write_thread = threading.Thread(target=write_ts_data)
        write_thread.start()

        # 使用线程池进行多线程下载
        with concurrent.futures.ThreadPoolExecutor(max_workers=self.threads) as executor:
            futures = []
            for index, segment in enumerate(segments):
                future = executor.submit(download_and_enqueue, index, segment)
                futures.append(future)

            # 等待所有下载任务完成
            concurrent.futures.wait(futures)

        # 等待写入线程完成
        write_thread.join()
        progress_bar.close()
        logging.info(f"m3u8 视频下载完成，保存路径: {save_path}")


    def ts_download_de(self,segment:str,key_method:str,key_uri:str,key_iv:str):
        """
        下载 ts 文件。
        功能：
            1. 下载 ts 文件。
            2. 解密 ts 文件。
        :param segment:
        :param key_method:
        :param key_uri:
        :param key_iv:
        :param file_name:
        :return:
        """
        # logging.info(f"调用下载 ts 文件:{segment}")
        file_name = os.path.join(self.out_dir,segment.split('/')[-1])
        retries = 0
        max_retries = 3
        key = None
        if key_method and key_method.upper() == 'AES-128' and key_uri:
            try:
                key_response = axios.get(key_uri)
                key_response.raise_for_status()
                key = key_response.content
                if key_iv and key_iv.startswith('0x'):
                    key_iv = bytes.fromhex(key_iv[2:])
            except requests.exceptions.RequestException as e:
                logging.error(f"获取加密密钥失败: {e}")
                return None
        while retries < max_retries:
            try:
                response = axios.get(segment)
                response.raise_for_status()
                ts_data = io.BytesIO()
                first_chunk = True

                if key:
                    if not key_iv:
                        key_iv = b'\x00' * 16
                    cipher = AES.new(key, AES.MODE_CBC, key_iv)
                    for chunk in response.iter_content(chunk_size=1024 * 16):
                        if chunk:
                            decrypted_chunk = cipher.decrypt(chunk)
                            if first_chunk:
                                if len(decrypted_chunk) >= 1 and decrypted_chunk[0] != 0x47:
                                    logging.warning(f"{segment} 不是有效的 TS 数据流")
                                    return None
                                first_chunk = False
                            ts_data.write(decrypted_chunk)
                else:
                    for chunk in response.iter_content(chunk_size=1024 * 16):
                        if chunk:
                            if first_chunk:
                                if len(chunk) >= 1 and chunk[0] != 0x47:
                                    logging.warning(f"{segment} 不是有效的 TS 数据流")
                                    return None
                                first_chunk = False
                            ts_data.write(chunk)
                return ts_data.getvalue()
                # ts_bytes = ts_data.getvalue()
                # with open(file_name, 'wb') as f:
                #     f.write(ts_bytes)
            except axiosError as e:
                logging.error(f"下载 TS 文件 {segment} 时发生错误: {e}")
                retries += 1
                time.sleep(1)
        logging.error(f"下载 {segment} 失败，已达到最大重试次数 {max_retries}")
        return None


    def m3u8_parse(self,url:str):
        """
        解析 m3u8 文件，获取所有分片的 URL。
        功能：
            1. 解析 M3U8 文件，返回包含 TS 片段链接、加密信息的字典。
            2. 检查 #EXT-X-STREAM-INF 字段，打印所有完整的 M3U8 链接，并默认选择下载画质最高的 M3U8 流视频链接。
            3. 返回包含 TS 片段链接、加密信息的字典。
            4. 处理解析过程中的错误和异常。
        :param url:
        :return:{}
        """
        segments: List[str] = []
        key_method: Optional[str] = None
        key_uri: Optional[str] = None
        key_iv: Optional[str] = None
        stream_infs: List[Dict] = []  # 存储 #EXT-X-STREAM-INF 信息
        base_url = url.rsplit('/', 1)[0] + '/'
        try:
            response = axios.get(url)
            m3u8_content = response.text
            lines = m3u8_content.splitlines()
            for i, line in enumerate(lines):
                if line.startswith('#EXT-X-KEY'):
                    method_match = re.search(r'METHOD=([^,]+)', line)
                    uri_match = re.search(r'URI="([^"]+)"', line)
                    iv_match = re.search(r'IV=([^,]+)', line)

                    if method_match:
                        key_method = method_match.group(1)
                    if uri_match:
                        key_uri = uri_match.group(1)
                        if not key_uri.startswith(('http://', 'https://')):
                            key_uri = urljoin(base_url, key_uri)
                    if iv_match:
                        key_iv = iv_match.group(1)
                elif line.startswith('#EXT-X-STREAM-INF'):
                    bandwidth_match = re.search(r'BANDWIDTH=(\d+)', line)
                    bandwidth = int(bandwidth_match.group(1)) if bandwidth_match else 0
                    next_line = lines[i + 1]
                    if next_line.lower().endswith('.m3u8'):
                        if not next_line.startswith(('http://', 'https://')):
                            next_line = urljoin(base_url, next_line)
                        stream_infs.append({
                            'bandwidth': bandwidth,
                            'url': next_line
                        })
                        print(f"找到 M3U8 链接: {next_line}, 带宽: {bandwidth}")
                elif line.startswith('#EXTINF'):
                    segment_url = lines[i + 1]
                    if not segment_url.startswith(('http://', 'https://')):
                        segment_url = urljoin(base_url, segment_url)
                    segments.append(segment_url)
            if stream_infs:
                # 按带宽从大到小排序，选择画质最高的链接
                highest_quality_stream = max(stream_infs, key=lambda x: x['bandwidth'])
                highest_quality_url = highest_quality_stream['url']
                print(f"选择最高画质 M3U8 链接: {highest_quality_url}")
                # 递归解析最高画质的 M3U8 文件
                return self.m3u8_parse(highest_quality_url)
            return {
                "segments": segments,
                "key_method": key_method,
                "key_uri": key_uri,
                "key_iv": key_iv
            }
        except axiosError as e:
            logging.error(f"解析 M3U8 文件: {url} 出错: {e}")
        return {
            "segments": [],
            "key_method": None,
            "key_uri": None,
            "key_iv": None
        }

    def download(self,url:str):
        """
        下载任务的调用函数。
        功能：
            1. 获取url指向文件的信息。
            2. 根据文件类型选择合适的下载方式。
        逻辑：
            1. 如果是html文件，解析html页面，获取所有媒体文件链接，返回一个集合。
            2. 如果是m3u8文件，尝试解析m3u8，并下载。
            3. 如果是其他文件，直接下载。
        :param url:
        :return:
        """
        file_info = self.get_file_info(url)
        logging.info(f"获取文件信息:{file_info}")

        if file_info['file_type'] == '.html':
            meta_links_set = self.html_parse(url)
            if meta_links_set:
                logging.info(f"未从{url}获取到媒体文件")
            for meta_link in meta_links_set:
                self.download(meta_link)
        elif file_info['file_type'] == '.m3u8':
            self.m3u8_download(file_info)
        else:
            self.file_download(file_info)

    def ts_to_mp4(self,ts_file_path,mp4_file_path):
        """
        将本地ts文件重新编码为mp4格式文件
        :param ts_file_path: TS 文件的路径
        :param mp4_file_path: 转换后的 MP4 文件的保存路径
        :return: 转换成功返回 True，失败返回 False
        """
        try:
            # 获取 TS 文件的时长
            duration_cmd = [
                'ffmpeg',
                '-i', ts_file_path,
                '-f', 'null',
                '-'
            ]
            try:
                duration_output = subprocess.check_output(duration_cmd, stderr=subprocess.STDOUT).decode('utf-8',
                                                                                                         errors='ignore')
                duration_match = re.search(r'Duration: (\d{2}):(\d{2}):(\d{2})', duration_output)
                if duration_match:
                    hours = int(duration_match.group(1))
                    minutes = int(duration_match.group(2))
                    seconds = int(duration_match.group(3))
                    total_seconds = hours * 3600 + minutes * 60 + seconds
                else:
                    logging.warning("无法获取 TS 文件时长，进度条可能不准确。")
                    total_seconds = 0
            except Exception as e:
                logging.warning(f"获取 TS 文件时长失败: {e}，进度条可能不准确。")
                total_seconds = 0

            # 构建 ffmpeg 命令，使用 H.264 编码视频，AAC 编码音频
            cmd = [
                'ffmpeg',
                '-i', ts_file_path,
                '-c:v', 'libx264',  # 使用 H.264 视频编码
                '-preset', 'fast',  # 编码速度预设，fast 可在速度和质量间取得平衡
                '-crf', '23',  # 视频质量参数，取值 0 - 51，数值越小质量越高，23 为默认平衡值
                '-c:a', 'aac',  # 使用 AAC 音频编码
                '-b:a', '128k',  # 音频比特率
                '-movflags', '+faststart',  # 让视频元数据位于文件开头，便于网页播放
                '-progress', 'pipe:1',  # 将进度信息输出到标准输出
                mp4_file_path
            ]

            # 初始化进度条
            progress_bar = tqdm(total=total_seconds, unit='s', desc="TS 转 MP4 进度")

            # 执行 ffmpeg 命令
            process = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

            current_time = 0
            while True:
                output = process.stdout.readline()
                if output == '' and process.poll() is not None:
                    break
                if output:
                    match = re.search(r'time=(\d{2}):(\d{2}):(\d{2})', output)
                    if match:
                        hours = int(match.group(1))
                        minutes = int(match.group(2))
                        seconds = int(match.group(3))
                        new_time = hours * 3600 + minutes * 60 + seconds
                        if new_time > current_time:
                            progress_bar.update(new_time - current_time)
                            current_time = new_time

            # 关闭进度条
            progress_bar.close()

            # 检查返回码
            if process.returncode == 0:
                logging.info(f"TS 文件 {ts_file_path} 成功重新编码转换为 MP4 文件 {mp4_file_path}")
                return True
            else:
                logging.error(f"转换失败，ffmpeg 错误信息: {process.stdout.read()}")
                return False

        except FileNotFoundError:
            logging.error("未找到 ffmpeg 可执行文件，请检查 ffmpeg 是否正确安装并配置了环境变量。")
            return False
        except Exception as e:
            logging.error(f"转换过程中出现未知错误: {str(e)}")
            return False

def main():
    """
    主函数，解析命令行参数并执行下载操作。
    功能：
        1. 解析命令行参数，包括 URL、保存目录、线程数和分片大小。
        2. 检查 URL 是否为 HTML 页面，如果是，则提取页面中的所有文件链接。
        3. 检查保存目录是否存在，如果不存在则创建。
    :return:
    """
    parser = argparse.ArgumentParser(description='文件下载器')
    # 创建互斥参数组
    group = parser.add_mutually_exclusive_group(required=True)
    # 第一个参数及其选项
    group.add_argument('--to4',metavar='ts_path', help='将ts文件转码为MP4，必须包含ts文件路径，输出文件为*.mp4')
    parser.add_argument('--mp4_path',help='mp4文件保存路径')
    # 第二个参数及其选项
    group.add_argument('--url', metavar='url', help='要下载文件的 URL 或 HTML 页面 URL')
    parser.add_argument('--dir', default='./temp_download', help='保存文件的目录，默认temp_download')
    parser.add_argument('--threads', type=int, default=math.ceil(psutil.cpu_count() / 2),
                        help='线程数，默认是 CPU 核心数的一半')
    parser.add_argument('--chunk-size', type=int, default=500 * 1024,
                        help='分片大小，单位为字节，默认 500KB')

    args = parser.parse_args()
    down = Downloader(out_dir=args.dir, threads=args.threads, chunk_size=args.chunk_size)
    if args.to4:
        mp4_path = args.mp4_path if args.mp4_path else args.to4+'.mp4'
        down.ts_to_mp4(args.to4,mp4_path)
    else:
        down.download(args.url)


if __name__ == "__main__":
    s = time.time()
    main()
    print("程序运行时间: {:.10f} 秒".format(time.time() - s))