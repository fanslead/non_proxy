#import <Foundation/Foundation.h>
#import <SafariServices/SafariServices.h>

#import "NonProxySafariStateBridge.h"

void np_query_safari_extension_state(
    const char *extension_identifier,
    np_safari_state_callback callback,
    void *context
) {
    if (callback == NULL) {
        return;
    }
    if (extension_identifier == NULL) {
        callback(false, false, "Safari 扩展 Bundle ID 无效。", context);
        return;
    }
    NSString *identifier = [NSString
        stringWithUTF8String:extension_identifier];
    if (identifier == nil || identifier.length == 0) {
        callback(false, false, "Safari 扩展 Bundle ID 无效。", context);
        return;
    }

    [SFSafariExtensionManager
        getStateOfSafariExtensionWithIdentifier:identifier
        completionHandler:^(
            SFSafariExtensionState *state,
            NSError *error
        ) {
            const char *message = error == nil
                ? NULL
                : error.localizedDescription.UTF8String;
            callback(
                state != nil && error == nil,
                state != nil && state.enabled,
                message,
                context
            );
        }];
}
