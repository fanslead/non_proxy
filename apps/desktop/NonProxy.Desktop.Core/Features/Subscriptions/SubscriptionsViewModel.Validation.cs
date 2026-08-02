using System.Text;

namespace NonProxy.Desktop.Core.Features.Subscriptions;

public sealed partial class SubscriptionsViewModel
{
    private bool ValidateEditor()
    {
        DisplayName = DisplayName.Trim();
        var endpoint = EndpointUrl.Trim();
        if (endpoint.Length == 0 && IsEditing)
        {
            ValidationMessage = null;
            return true;
        }
        if (Encoding.UTF8.GetByteCount(endpoint) > 4 * 1024
            || !Uri.TryCreate(endpoint, UriKind.Absolute, out var uri)
            || uri.Scheme != Uri.UriSchemeHttps
            || string.IsNullOrEmpty(uri.Host)
            || !string.IsNullOrEmpty(uri.UserInfo)
            || !string.IsNullOrEmpty(uri.Fragment))
        {
            ValidationMessage = "请输入完整的 HTTPS 订阅地址，且不要包含账号信息或片段。";
            return false;
        }

        EndpointUrl = endpoint;
        ValidationMessage = null;
        return true;
    }
}
