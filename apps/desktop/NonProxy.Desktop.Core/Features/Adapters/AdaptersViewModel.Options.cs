using NonProxy.Adapter.V1;

namespace NonProxy.Desktop.Core.Features.Adapters;

public sealed partial class AdaptersViewModel
{
    public static IReadOnlyList<AdapterClientOption> ClientOptions { get; } =
    [
        new(
            "Surge for Mac",
            AdapterClient.Surge,
            "surge-primary",
            "nonproxy.list",
            "选择 Surge.app 或其中的 surge-cli 可执行文件。",
            "选择当前正在使用的 Surge 配置文件。",
            "Surge 固定使用 DIRECT，通常留空即可。"),
        new(
            "Clash / Mihomo",
            AdapterClient.Mihomo,
            "mihomo-primary",
            "nonproxy.yaml",
            "选择当前客户端实际运行的 Mihomo 可执行文件。",
            "选择包含 external-controller 的当前主配置。",
            "Mihomo 固定使用 DIRECT，通常留空即可。"),
        new(
            "sing-box",
            AdapterClient.SingBox,
            "sing-box-primary",
            "nonproxy.json",
            "选择当前客户端实际运行的 sing-box 可执行文件。",
            "选择唯一运行进程正在使用的主配置。",
            "存在多个 direct outbound 时，填写要使用的 outbound tag。"),
    ];
}
