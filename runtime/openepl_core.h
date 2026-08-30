/* libopenepl_core — Phase 0 spike runtime (PRD G3).
 *
 * A portable C reimplementation of a *tiny* slice of EPL's core library
 * (`krnln`), organized one command per translation unit so the system linker's
 * dead-stripping (`--gc-sections`) pulls in only referenced commands — the
 * BlackMoon "fragment extraction" property (PRD D3).
 *
 * NOTE (open question D6): the eventual runtime language (C vs C++ vs Rust) is
 * unresolved.  C is used here to keep the clang link step free of std/runtime
 * baggage during the spike; nothing about the ABI below assumes C.
 */
#ifndef OPENEPL_CORE_H
#define OPENEPL_CORE_H

/* Program entry emitted by the backend (PRD §1.4 lean-entry model). */
extern int ECodeStart(void);

/* Runtime lifecycle. */
void E_Init(void);        /* acquire process resources; init linked libraries */
void E_DestroyRes(void);  /* release them (no-op in the spike) */

/* Core commands (the two the vertical slice needs). */
void oe_print_int(int value);          /* SDT_INT  -> stdout + newline */
void oe_print_text(const char *text);  /* SDT_TEXT -> stdout + newline (NULL = empty) */

#endif /* OPENEPL_CORE_H */
