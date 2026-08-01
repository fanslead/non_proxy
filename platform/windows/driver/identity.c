#include "nonproxy_wfp_driver.h"

BOOLEAN
NonProxyReadPackageSid(
    _In_ const FWP_VALUE0* Value,
    _Outptr_result_bytebuffer_(*PackageSidLength) const UCHAR** PackageSid,
    _Out_ UINT32* PackageSidLength)
{
    ULONG length;

    *PackageSid = NULL;
    *PackageSidLength = 0;
    if (Value->type == FWP_EMPTY) {
        return TRUE;
    }
    if (Value->type != FWP_SID) {
        return FALSE;
    }
    if (Value->sid == NULL) {
        return TRUE;
    }
    if (!RtlValidSid(Value->sid)) {
        return FALSE;
    }
    length = RtlLengthSid(Value->sid);
    if (length == 0 || length > NP_WFP_MAX_PACKAGE_SID_BYTES) {
        return FALSE;
    }
    *PackageSid = (const UCHAR*)Value->sid;
    *PackageSidLength = length;
    return TRUE;
}
