import Dispatch
import NetworkExtension
import NonProxyTransparentProxy

// 显式引用 Provider 类型，防止发布构建移除仅由系统按类名加载的实现。
_ = TransparentProxyProvider.self
NEProvider.startSystemExtensionMode()
dispatchMain()
