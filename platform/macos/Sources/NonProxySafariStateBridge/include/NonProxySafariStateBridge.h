#ifndef NONPROXY_SAFARI_STATE_BRIDGE_H
#define NONPROXY_SAFARI_STATE_BRIDGE_H

#include <stdbool.h>

typedef void (*np_safari_state_callback)(
    bool available,
    bool enabled,
    const char *error_message,
    void *context
);

void np_query_safari_extension_state(
    const char *extension_identifier,
    np_safari_state_callback callback,
    void *context
);

#endif
