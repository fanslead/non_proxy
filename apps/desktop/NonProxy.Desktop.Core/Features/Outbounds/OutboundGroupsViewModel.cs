using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NonProxy.Desktop.Core.Services.Control;

namespace NonProxy.Desktop.Core.Features.Outbounds;

public sealed partial class OutboundGroupsViewModel : ObservableObject
{
    private readonly IOutboundGroupService _service;
    private ulong _routingRevision;
    private string? _editingId;
    private ulong? _editingRevision;
    private Dictionary<string, OutboundListItem> _outboundsById =
        new Dictionary<string, OutboundListItem>(StringComparer.Ordinal);

    [ObservableProperty]
    private bool _isBusy;

    [ObservableProperty]
    private bool _isEditorVisible;

    [ObservableProperty]
    private string _editingName = string.Empty;

    [ObservableProperty]
    private string? _validationMessage;

    [ObservableProperty]
    private string? _operationMessage;

    [ObservableProperty]
    private OutboundGroupListItem? _pendingDeleteGroup;

    [ObservableProperty]
    private bool _isAvailable;

    public OutboundGroupsViewModel(IOutboundGroupService service)
    {
        _service = service;
        StartCreateCommand = new RelayCommand(StartCreate, CanStartCreate);
        EditCommand = new RelayCommand<OutboundGroupListItem>(Edit, CanEdit);
        CancelEditCommand = new RelayCommand(CancelEdit);
        AddMemberCommand = new RelayCommand<OutboundListItem>(AddMember, CanAddMember);
        RemoveMemberCommand = new RelayCommand<OutboundGroupMemberItem>(
            RemoveMember,
            CanRemoveMember);
        MoveMemberUpCommand = new RelayCommand<OutboundGroupMemberItem>(
            MoveMemberUp,
            CanMoveMemberUp);
        MoveMemberDownCommand = new RelayCommand<OutboundGroupMemberItem>(
            MoveMemberDown,
            CanMoveMemberDown);
        SaveCommand = new AsyncRelayCommand(SaveAsync, CanSave);
        SetDefaultCommand = new AsyncRelayCommand<OutboundGroupListItem>(
            SetDefaultAsync,
            CanSetDefault);
        RequestDeleteCommand = new RelayCommand<OutboundGroupListItem>(
            group => PendingDeleteGroup = group,
            group => group is { IsDefault: false } && !IsBusy);
        CancelDeleteCommand = new RelayCommand(() => PendingDeleteGroup = null);
        ConfirmDeleteCommand = new AsyncRelayCommand(
            ConfirmDeleteAsync,
            () => PendingDeleteGroup is not null && !IsBusy);
    }

    public ObservableCollection<OutboundGroupListItem> Groups { get; } = [];

    public ObservableCollection<OutboundListItem> AvailableOutbounds { get; } = [];

    public ObservableCollection<OutboundGroupMemberItem> PriorityMembers { get; } = [];

    public IRelayCommand StartCreateCommand { get; }

    public IRelayCommand<OutboundGroupListItem> EditCommand { get; }

    public IRelayCommand CancelEditCommand { get; }

    public IRelayCommand<OutboundListItem> AddMemberCommand { get; }

    public IRelayCommand<OutboundGroupMemberItem> RemoveMemberCommand { get; }

    public IRelayCommand<OutboundGroupMemberItem> MoveMemberUpCommand { get; }

    public IRelayCommand<OutboundGroupMemberItem> MoveMemberDownCommand { get; }

    public IAsyncRelayCommand SaveCommand { get; }

    public IAsyncRelayCommand<OutboundGroupListItem> SetDefaultCommand { get; }

    public IRelayCommand<OutboundGroupListItem> RequestDeleteCommand { get; }

    public IRelayCommand CancelDeleteCommand { get; }

    public IAsyncRelayCommand ConfirmDeleteCommand { get; }

    public bool HasGroups => Groups.Count > 0;

    public bool HasAvailableOutbounds => AvailableOutbounds.Count > 0;

    public bool IsDeleteConfirmationVisible => PendingDeleteGroup is not null;

    public string EditorTitle => _editingRevision is null
        ? "创建自动切换线路组"
        : "编辑自动切换线路组";

    public event EventHandler<OutboundGroupDefaultRouteChange>?
        DefaultRouteChanged;

    partial void OnEditingNameChanged(string value)
    {
        ValidationMessage = null;
        SaveCommand.NotifyCanExecuteChanged();
    }

    partial void OnPendingDeleteGroupChanged(OutboundGroupListItem? value)
    {
        OnPropertyChanged(nameof(IsDeleteConfirmationVisible));
        ConfirmDeleteCommand.NotifyCanExecuteChanged();
    }

    partial void OnIsBusyChanged(bool value)
    {
        NotifyCommands();
    }

    public void ApplyCatalog(
        OutboundGroupCatalog catalog,
        IReadOnlyList<OutboundListItem> outbounds)
    {
        ArgumentNullException.ThrowIfNull(catalog);
        ArgumentNullException.ThrowIfNull(outbounds);
        _routingRevision = catalog.RoutingRevision;
        IsAvailable = catalog.RoutingRevision > 0;
        _outboundsById = outbounds.ToDictionary(
            outbound => outbound.Id,
            StringComparer.Ordinal);
        Groups.Clear();
        foreach (var group in catalog.Groups.OrderBy(group => group.Name))
        {
            Groups.Add(WithMemberNames(group));
        }
        AvailableOutbounds.Clear();
        foreach (var outbound in outbounds
                     .Where(outbound => outbound.SupportsDefaultRoute)
                     .OrderBy(outbound => outbound.Name))
        {
            AvailableOutbounds.Add(outbound);
        }
        PendingDeleteGroup = null;
        CancelEdit();
        OnPropertyChanged(nameof(HasGroups));
        OnPropertyChanged(nameof(HasAvailableOutbounds));
        NotifyCommands();
    }

    private bool CanStartCreate()
    {
        return IsAvailable && !IsBusy && AvailableOutbounds.Count >= 2;
    }

    private void StartCreate()
    {
        _editingId = $"failover-{Guid.NewGuid():N}";
        _editingRevision = null;
        EditingName = "自动切换线路";
        PriorityMembers.Clear();
        OpenEditor();
    }

    private bool CanEdit(OutboundGroupListItem? group)
    {
        return group is not null && IsAvailable && !IsBusy;
    }

    private void Edit(OutboundGroupListItem? group)
    {
        if (!CanEdit(group))
        {
            return;
        }
        _editingId = group!.Id;
        _editingRevision = group.Revision;
        EditingName = group.Name;
        PriorityMembers.Clear();
        foreach (var id in group.OutboundIds)
        {
            _outboundsById.TryGetValue(id, out var outbound);
            PriorityMembers.Add(OutboundGroupMemberItem.Create(
                PriorityMembers.Count + 1,
                id,
                outbound));
        }
        OpenEditor();
    }

    private void OpenEditor()
    {
        ValidationMessage = null;
        OperationMessage = null;
        IsEditorVisible = true;
        OnPropertyChanged(nameof(EditorTitle));
        NotifyCommands();
    }

    private void CancelEdit()
    {
        IsEditorVisible = false;
        _editingId = null;
        _editingRevision = null;
        EditingName = string.Empty;
        PriorityMembers.Clear();
        ValidationMessage = null;
        OnPropertyChanged(nameof(EditorTitle));
        NotifyCommands();
    }

    private bool CanAddMember(OutboundListItem? outbound)
    {
        return outbound is not null
            && IsEditorVisible
            && !IsBusy
            && PriorityMembers.Count < 32
            && PriorityMembers.All(member => !string.Equals(
                member.OutboundId,
                outbound.Id,
                StringComparison.Ordinal));
    }

    private void AddMember(OutboundListItem? outbound)
    {
        if (!CanAddMember(outbound))
        {
            return;
        }
        PriorityMembers.Add(OutboundGroupMemberItem.Create(
            PriorityMembers.Count + 1,
            outbound!.Id,
            outbound));
        MembersChanged();
    }

    private bool CanRemoveMember(OutboundGroupMemberItem? member)
    {
        return member is not null && IsEditorVisible && !IsBusy;
    }

    private void RemoveMember(OutboundGroupMemberItem? member)
    {
        if (member is not null && PriorityMembers.Remove(member))
        {
            ReindexMembers();
        }
    }

    private bool CanMoveMemberUp(OutboundGroupMemberItem? member)
    {
        return member is not null
            && !IsBusy
            && PriorityMembers.IndexOf(member) > 0;
    }

    private void MoveMemberUp(OutboundGroupMemberItem? member)
    {
        MoveMember(member, -1);
    }

    private bool CanMoveMemberDown(OutboundGroupMemberItem? member)
    {
        var index = member is null ? -1 : PriorityMembers.IndexOf(member);
        return !IsBusy && index >= 0 && index < PriorityMembers.Count - 1;
    }

    private void MoveMemberDown(OutboundGroupMemberItem? member)
    {
        MoveMember(member, 1);
    }

    private void MoveMember(OutboundGroupMemberItem? member, int offset)
    {
        if (member is null)
        {
            return;
        }
        var index = PriorityMembers.IndexOf(member);
        var target = index + offset;
        if (index < 0 || target < 0 || target >= PriorityMembers.Count)
        {
            return;
        }
        PriorityMembers.Move(index, target);
        ReindexMembers();
    }

    private void ReindexMembers()
    {
        for (var index = 0; index < PriorityMembers.Count; index++)
        {
            PriorityMembers[index] = PriorityMembers[index] with
            {
                Position = index + 1,
            };
        }
        MembersChanged();
    }

    private void MembersChanged()
    {
        ValidationMessage = null;
        NotifyCommands();
    }

    private OutboundGroupListItem WithMemberNames(OutboundGroupListItem group)
    {
        return group with
        {
            MemberNames = group.OutboundIds.Select(id =>
                _outboundsById.TryGetValue(id, out var outbound)
                    ? outbound.Name
                    : id).ToArray(),
        };
    }

}
