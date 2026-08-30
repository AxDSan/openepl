/* Component descriptors, read from the UI library's design-time metadata.
 *
 * The designer links `ui_libinfo.c` directly — the metadata translation unit
 * finally meeting its intended consumer. That is the `.fne` design-time /
 * `.fnr` runtime split (D12) working exactly as designed: the same table the
 * compiler introspects tells the designer what a toolbox holds, which
 * properties an inspector shows, and which events can be wired.
 */
#ifndef OPENEPL_DESIGNER_DESCRIPTORS_H
#define OPENEPL_DESIGNER_DESCRIPTORS_H

#include "openepl_abi.h"

namespace openepl::designer {

inline const OpenEPL_LibInfo* ui_library() { return openepl_get_lib_info(); }

inline const OpenEPL_ComponentDesc* describe(const char* type_name) {
    const OpenEPL_LibInfo* lib = ui_library();
    for (int i = 0; i < lib->component_count; i++) {
        if (std::strcmp(lib->components[i].name, type_name) == 0) return &lib->components[i];
    }
    return nullptr;
}

} // namespace openepl::designer
#endif
