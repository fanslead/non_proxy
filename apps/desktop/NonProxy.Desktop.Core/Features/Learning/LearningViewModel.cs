using NonProxy.Desktop.Core.Features.Common;

namespace NonProxy.Desktop.Core.Features.Learning;

public sealed partial class LearningViewModel : LoadableViewModel
{
    public LearningViewModel()
        : base("智能学习")
    {
    }

    protected override Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.CompletedTask;
    }
}
