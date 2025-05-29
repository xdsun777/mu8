"""
此模块实现了自定义请求配置和重试策略，用于发送 HTTP 请求。
主要功能：
- 定义了 Config 类，用于管理通用的请求配置，包括 User-Agent、代理 IP 和通用请求头。
- 定义了 Request 类，用于发送 HTTP 请求。
- 配置了 HTTP 适配器，包括连接池数量、最大连接数和重试策略。
- 提供了 get_random_headers 和 get_random_proxy 方法，用于随机获取 User-Agent 和代理 IP。
使用方法：
1. 导入 Config 和 Request 类。
2. 使用 Config.get_random_headers() 获取随机的请求头。
3. 使用 Config.get_random_proxy() 获取随机的代理 IP。
4. 使用 Request 类发送 HTTP 请求。
示例：
from Config import Config
from Request import Request
headers = Config.get_random_headers()
"""
from http.cookiejar import DefaultCookiePolicy
from requests.adapters import HTTPAdapter
from urllib3 import Retry
import requests
import certifi
import time
import random
class AxiosError(Exception):
    """自定义下载异常类。"""
    def __init__(self, message, status_code=None):
        super().__init__(message)
        self.status_code = status_code
        self.message = message
        self.name = 'AxiosError'

    def __str__(self):
        return f"{self.name}: {self.message}"
    def __repr__(self):
        return f"{self.name}({self.message}, status_code={self.status_code})"


class Config:
    # 定义多个 User-Agent，用于动态更换
    _USER_AGENTS = [
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36"
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36 Edg/135.0.0.0",
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:120.0) Gecko/20100101 Firefox/120.0",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0",
        "Mozilla/5.0 (iPad; CPU OS 17_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.1 Mobile/15E148 Safari/604.1"
    ]
    # 代理 IP 列表，可根据实际情况添加
    _PROXIES = [
        # 示例格式，需替换为真实可用代理
        # {"http": "http://user:pass@ip:port", "https": "http://user:pass@ip:port"}
    ]
    # 定义通用请求头
    COMMON_HEADERS = {
        "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7",
        "Accept-Encoding": "gzip, deflate, br, zstd",
        "Accept-Language": "zh,zh-TW;q=0.9,en-US;q=0.8,en;q=0.7,zh-CN;q=0.6",
        "Connection": "keep-alive",
        "Referer": "https://www.google.com/",
        "Upgrade-Insecure-Requests": "1",
        "Sec-Fetch-User": "?1",
        "Sec-Fetch-Dest": "document",
        "Sec-Fetch-Mode": "navigate",
        "Sec-Fetch-Site": "cross-site",
        "Pragma": "no-cache",
        "Cache-Control": "no-cache"
    }

    @classmethod
    def get_random_headers(cls):
        headers = cls.COMMON_HEADERS.copy()
        headers["User-Agent"] = random.choice(cls._USER_AGENTS)
        return headers
    @classmethod
    def get_random_proxy(cls):
        if cls._PROXIES:
            return random.choice(cls._PROXIES)
        return None
    @classmethod
    def get_headers(cls):
        headers = cls.COMMON_HEADERS.copy()
        headers["User-Agent"] = cls._USER_AGENTS[0]
        return headers
    def __init__(self,**kwargs):
        # 创建实例属性，初始化为类属性的副本
        self.headers = self.COMMON_HEADERS.copy()
        # 使用传入的关键字参数更新实例属性
        if kwargs:
            self.headers.update(kwargs)
    def __call__(self, *args, **kwargs):
        return {
            **self.headers,
            "User-Agent": random.choice(self._USER_AGENTS),
        }


class Request:
    _retry = Retry(
        total=10,  # 最大重试次数
        connect=6,  # 连接失败时的重试次数
        read=5,  # 读取失败时的重试次数
        redirect=3,  # 重定向的最大次数
        status_forcelist=[500, 502, 503, 504],  # 需要重试的 HTTP 状态码
        backoff_factor=0.5  # 重试间隔的退避因子
    )
    # 配置适配器
    _adapter = HTTPAdapter(
        pool_connections=77,  # 连接池数量
        pool_maxsize=77,  # 每个连接池的最大连接数
        max_retries=_retry  # 重试策略
    )
    _session = requests.Session()
    # 挂载适配器
    _session.mount('http://', _adapter)
    _session.mount('https://', _adapter)
    # 更新请求头
    _session.headers.update(Config().get_random_headers())
    # 配置 SSL 验证
    _session.verify = certifi.where()
    # 配置 Cookie 策略
    _session.cookies.set_policy(DefaultCookiePolicy(rfc2965=True))


    def __init__(self, url=None,method='GET', *args, **kwargs):
        self.url = url
        self.method = method
        self.args = args
        self.kwargs = kwargs #// Path: react-ts-playground/src/components/Button/Button.stories.tsx
    def __call__(self, *args, **kwargs):
        return self.__class__._request(self.method,self.url, *args, **kwargs)

    def text(self, *args, **kwargs):
        return self(*args, **kwargs).text
    def status_code(self, *args, **kwargs):
        return self(*args, **kwargs).status_code

    @classmethod
    def _request(cls, method, url, *args, **kwargs):
        kwargs.setdefault("timeout", (5, 15))
        match method:
            case 'HEAD':
                return cls.head(url, *args, **kwargs)
            case 'POST':
                return cls.post(url, *args, **kwargs)
            case _:
                return cls.get(url, *args, **kwargs)


    @classmethod
    def head(cls, url, *args, **kwargs):
        return cls._session.head(url, *args, **kwargs)
    @classmethod
    def get(cls, url, *args,**kwargs):
        return cls._session.get(url, *args, **kwargs)
    @classmethod
    def post(cls, url, *args,**kwargs):
        return cls._session.post(url, *args, **kwargs)


if __name__ == "__main__":
    start = time.time()
    # rs = Request(url='http://httpbin.org/get', method='GET')
    # print(rs.get('http://httpbin.org/get'))
    # 上下两种方式等价
    # rs = Request.get('http://www.baidu.com')
    # print(rs.text)
    cookies = {
        "starstruck_d3e123bf01ade4aa9f91314f509efadf": "312d180a09271f6c38bdab640c1abbfd",
        "UGVyc2lzdFN0b3JhZ2U": "%7B%7D",
        "__PPU_puid": "16721746548958261472"
    }
    axios = Request()
    rs = axios.get('https://www.movieffm.net/drama/338824/',cookies=cookies)
    print(rs.text)
    #
    # print(time.time() - start)