#ifndef WXD_MISC_H
#define WXD_MISC_H

#include "../wxd_types.h"

// --- Miscellaneous System Functions ---

// Produces an audible beep sound
WXD_EXPORTED void
wxd_Bell(void);

// Opens the given URL in the default browser
// Returns true if the browser was successfully launched, false otherwise
WXD_EXPORTED bool
wxd_LaunchDefaultBrowser(const char* url, int flags);

// Returns true if the Control key is currently pressed
WXD_EXPORTED bool
wxd_IsControlKeyDown(void);

// Returns true if the platform command key is currently pressed
WXD_EXPORTED bool
wxd_IsCommandKeyDown(void);

#endif // WXD_MISC_H
