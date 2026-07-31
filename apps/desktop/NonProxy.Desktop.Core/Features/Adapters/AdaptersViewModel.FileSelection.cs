using NonProxy.Adapter.V1;

namespace NonProxy.Desktop.Core.Features.Adapters;

public sealed partial class AdaptersViewModel
{
    private Task ChooseExecutableAsync(CancellationToken cancellationToken)
    {
        return RunAdapterOperationAsync(
            async token =>
            {
                var selection = await _filePicker.PickExecutableAsync(
                    SelectedClient.Client,
                    SelectedClient.DisplayName,
                    token);
                if (selection.IsSelected)
                {
                    ExecutablePath = NormalizeSelectedExecutable(
                        selection.LocalPath!,
                        SelectedClient.Client);
                    OperationMessage =
                        "已选择客户端候选；登记时仍会验证真实版本、可执行文件和当前配置。";
                }
                else if (selection.Message is not null)
                {
                    OperationMessage = selection.Message;
                }
            },
            cancellationToken);
    }

    private Task ChooseConfigurationAsync(CancellationToken cancellationToken)
    {
        return RunAdapterOperationAsync(
            async token =>
            {
                var selection = await _filePicker.PickConfigurationAsync(
                    SelectedClient.Client,
                    SelectedClient.DisplayName,
                    token);
                if (selection.IsSelected)
                {
                    MainConfigurationPath = selection.LocalPath!;
                    OperationMessage =
                        "已选择主配置候选；同步前仍会确认它属于当前运行客户端。";
                }
                else if (selection.Message is not null)
                {
                    OperationMessage = selection.Message;
                }
            },
            cancellationToken);
    }

    private static string NormalizeSelectedExecutable(
        string path,
        AdapterClient client)
    {
        return client == AdapterClient.Surge
            && path.EndsWith(".app", StringComparison.OrdinalIgnoreCase)
                ? Path.Combine(path, "Contents", "Applications", "surge-cli")
                : path;
    }
}
