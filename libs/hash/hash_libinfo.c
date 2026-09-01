/* "hash" library metadata (design-time only; compiled into the introspection
 * .so, never a shipped program — the same split as core_libinfo.c).
 *
 * Digests and encodings. Not ciphers, not password hashing, not key
 * derivation — see the header comment of hash_cmds.c. */
#include "openepl_abi.h"

void hash_sha256(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void hash_sha1(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void hash_md5(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void hash_crc32(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void hash_hmac_sha256(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void base64_encode(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void base64_decode(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void hex_encode(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);
void hex_decode(OpenEPL_Slot *ret, int32_t argc, OpenEPL_Slot *argv);

static const int32_t P_T[]  = { OE_SDT_TEXT };
static const int32_t P_TT[] = { OE_SDT_TEXT, OE_SDT_TEXT };

static const OpenEPL_CommandDesc HASH_COMMANDS[] = {
    /* digests — all infallible, all lowercase hex */
    { "hash_sha256",      "hash_sha256",      OE_SDT_TEXT,  1, P_T  },
    { "hash_sha1",        "hash_sha1",        OE_SDT_TEXT,  1, P_T  },
    { "hash_md5",         "hash_md5",         OE_SDT_TEXT,  1, P_T  },
    { "hash_crc32",       "hash_crc32",       OE_SDT_INT64, 1, P_T  },
    { "hash_hmac_sha256", "hash_hmac_sha256", OE_SDT_TEXT,  2, P_TT },
    /* encodings — the two decoders are the only fallible commands here */
    { "base64_encode",    "base64_encode",    OE_SDT_TEXT,  1, P_T  },
    { "base64_decode",    "base64_decode",    OE_SDT_TEXT,  1, P_T  },
    { "hex_encode",       "hex_encode",       OE_SDT_TEXT,  1, P_T  },
    { "hex_decode",       "hex_decode",       OE_SDT_TEXT,  1, P_T  },
};

static const OpenEPL_LibInfo HASH_INFO = {
    OPENEPL_ABI_VERSION,
    "hash",
    "openepl-hash-7c31-9a02-44de-b81f5e620c11",
    0, 1, 0,
    (int32_t)(sizeof(HASH_COMMANDS) / sizeof(HASH_COMMANDS[0])),
    HASH_COMMANDS,
    0, NULL,   /* the hash library contributes no visual components */
};

const OpenEPL_LibInfo *openepl_get_lib_info(void) {
    return &HASH_INFO;
}
