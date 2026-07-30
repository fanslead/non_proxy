namespace NonProxy.Desktop.Core.Services.Control;

public sealed class ControlServiceException : Exception
{
    public ControlServiceException(
        string code,
        string userMessage,
        Exception? innerException = null)
        : base($"{code}: {userMessage}", innerException)
    {
        Code = code;
        UserMessage = userMessage;
    }

    public string Code { get; }

    public string UserMessage { get; }
}
