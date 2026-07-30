using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Features.Common;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Learning;

public sealed partial class LearningViewModel : LoadableViewModel
{
    private readonly ILearningService _learningService;

    [ObservableProperty]
    private LearningStatus _status = new(
        false,
        0,
        null,
        "学习模式尚未启动。");

    public LearningViewModel(ILearningService learningService)
        : base("智能学习")
    {
        _learningService = learningService;
        StartCommand = new AsyncRelayCommand(StartAsync);
        StopCommand = new AsyncRelayCommand(StopAsync);
    }

    public IAsyncRelayCommand StartCommand { get; }

    public IAsyncRelayCommand StopCommand { get; }

    protected override async Task LoadCoreAsync(CancellationToken cancellationToken)
    {
        Status = await _learningService.GetStatusAsync(cancellationToken);
    }

    private Task StartAsync(CancellationToken cancellationToken)
    {
        return RunOperationAsync(
            async token => Status = await _learningService.StartAsync(token),
            cancellationToken);
    }

    private Task StopAsync(CancellationToken cancellationToken)
    {
        return RunOperationAsync(
            async token => Status = await _learningService.StopAsync(token),
            cancellationToken);
    }
}
