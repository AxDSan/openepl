# The IDE

`openepl-studio` with no arguments opens the welcome screen; with a `.oir`
file, a `project.oeproj`, or a directory holding one, it opens that project.
The welcome screen shows the toolchain's version, the templates, an *Open
Project…* and *Open File…* browser, and the projects opened most recently.
Opening a project opens its `main:` file; Studio never reads the project file
itself — `openepl project` does.

![The OpenEPL Studio visual designer](./assets/screenshot-designer.png)

## The layout

- **Toolbox** — the components you can place, grouped by the section each kit
  declares. Search narrows it. A filled square is a control with a rectangle; a
  diamond is one without. Greyed items are declared but not implemented yet.
- **Canvas** — the form as it will look. Drag to move, drag the handles to
  resize (the pointer shows the direction), drag the form's corner to resize
  the window. Double-click a component to write its handler.
- **Tray** — the strip under the canvas, holding the components that have no
  pixels. Click one to select it.
- **Inspector** — every property of the selection, and its events on the
  second tab. Type a value and press Enter.
- **Code preview** — the source for whatever is selected. Click it to open the
  editor.
- **Problems** — what the language server has found, updated as you type,
  and the list of references when you ask for one.
- **Output** — build progress and whatever your program prints.

The panels are resizable: drag the dividers between them.

## The toolbox is what the toolchain reports

Studio asks `openepl kits` which kits are installed and `openepl commands`
what each one declares, every time it starts. Nothing about the palette is
compiled into the IDE, so a kit dropped into `kits/` beside your project — or
installed with `openepl kit add` — shows up in the toolbox with no new version
of Studio. The heading it files under is the `section` in the kit's `lib.json`.

## Property editors

The kind of editor a property gets comes from the component's own metadata,
not from the property's name:

| Declared editor | What the inspector offers |
|---|---|
| `color` | a swatch showing the current colour; click it for a palette |
| `file` | a **Browse…** button listing the files beside your project |
| `multiline` | a box several lines tall, for text with newlines in it |
| *(none)* | a one-line field |

A colour swatch with a dashed outline means the property is unset — the
default applies, and nothing is written to your file until you choose.

## Components without pixels

A `timer` has an interval and a `tick` event. An `action` has a caption and a
shortcut. An `httpserver` has a port. None of them has a rectangle, so none of
them can go on the form: they are declared beside it, at module level, and the
compiler rejects a form that tries to hold one.

Studio puts them in the tray under the canvas instead. Drop one from the
toolbox's **System** section and it is written after the form rather than
inside it; select it in the tray to edit its properties and wire its events
exactly as you would a button's.

A component from a kit also needs that kit in scope. Studio says so in the
output pane rather than writing a `use` line you did not ask for — add it in
the Code view.

A grid on the canvas shows its rows. When it is bound to a datasource
declared in the file, it shows that source's columns and rows, with the
selected row highlighted, exactly as it will when the program runs; a source
the file does not declare leaves a header naming it.

## Double-click writes the handler

Double-click a button and Studio opens the editor inside `sub
button1_click`. If no handler was wired, it wires one — `on click:
button1_click` in the component's block, and the subroutine at the end of
the file — and if one was, it just goes there.

The event is the component's default one: `click` for a button, `change` for
an editbox or a combobox, `tick` for a timer, `select` for a grid. It is not
a table in the IDE: it is the first event the component's own descriptor
declares, so a component from a kit gets the gesture too.

The subroutine takes what the event hands it. A grid's `select` hands the
row, so the stub reads `sub grid1_select(n: int)`; a button's `click` hands
nothing, so its stub takes nothing. Delete the parameter if you do not want
it — the compiler accepts a handler that declares the event's parameters or
none.

Double-clicking a component in the tray does the same for a timer or an
action.

## Completion, hover, definition, references

The editor asks the same language server that VS Code and Neovim use:

| Gesture | What happens |
|---|---|
| type a name | a completion popup: commands, components, a component's properties and events after `id.`, your own subroutines, locals, keywords |
| **Enter** / **Tab** on the popup | accept the highlighted entry; **Esc** dismisses it |
| rest the pointer on a name | a tip with its signature: a command's parameters, a subroutine's, a property's type and editor, what an event hands its handler |
| **F12** | jump to the declaration of the name at the caret |
| **Shift+F12** | list every use of the name at the caret in the Problems strip; click one to jump |

The popup narrows as you type without asking the server again, and closes when
the caret leaves the word — so it keeps up with a line still being written,
which is most of the time you are actually editing.

A command has no declaration to jump to — it is written in C, in the runtime
or a library — so F12 on one says so, and hover shows what a jump would have.

Diagnostics are underlined at the exact range the compiler reported, not the
whole line: two calls on one line and the bad one is marked.

## Designer and Code

The two tabs are two views of one file. Anything you do in the designer is
written to the source, and anything you write is read back by the designer.

![The code editor](./assets/screenshot-editor.png)

The editor has syntax highlighting and live diagnostics from the same language
server other editors use. **Ctrl+S** saves. Saving from the editor re-reads
the file, so the canvas follows what you wrote.

**Tab** indents by two spaces and **Return** keeps the indentation of the line
you were on, one level deeper after a line that opens a block — so a `sub`, an
`if` or a `form` lays itself out as you type. Nothing re-indents a file you
opened: the editor only ever indents the line you are writing.

Code that does not parse is still saved — the error appears in Problems rather
than blocking you. Losing what you typed because it is not finished yet would
be worse than a stale canvas.

## Running

**Run** builds the project and starts it. **Build Binary** builds without
running. **Stop** ends a running program.

The console shows the exact command, the stages of the build, every line the
compiler emits, how long it took, and the size of the result — then your
program's own output, and its exit code.

```text
> openepl build my-app/main.oir -o /tmp/openepl_studio_app
  stage 1/4  parse + validate .oir
  stage 2/4  lower to LLVM IR
  stage 3/4  clang: assemble + link the runtime
  stage 4/4  dead-strip unused commands (--gc-sections)
OK  /tmp/openepl_studio_app — 17 KB in 1.42s (clean native, no runtime unpack)
  LLVM IR kept beside it: /tmp/openepl_studio_app.ll
> running: /tmp/openepl_studio_app  (pid 12345)
  output below is the program's own stdout/stderr
Hello from OpenEPL.
> app exited with code 0
```

A console program has no window: it runs, prints into that pane, and finishes.

## Undo

**Ctrl+Z** and **Ctrl+Shift+Z**, or the toolbar buttons. Designer edits —
adding, moving, resizing, deleting, property changes — are undoable.

## Where projects go

Projects created from the welcome screen are made in the directory Studio was
started from. Start it where you keep your work:

```sh
cd ~/projects && openepl-studio
```

## Studio on Windows

Studio cross-builds for Windows x86-64 from Linux, the same way a windowed
program does: `designer/build.sh --os windows` (or `designer/build-windows.sh`)
writes `designer/windows/openepl-studio.exe` with mingw-w64's g++, links the
Windows build of RmlUi statically, and copies beside it the SDL2, SDL2_image
and freetype DLLs it imports — the list read from the images, as
`openepl build --os windows` reads it. `tools/package-windows.sh` assembles
that into `dist/openepl-<version>-windows-x86_64/` and a zip: `bin\`,
`templates\`, `runtime\`, `libs\`, `docs\`, the licences, and DejaVu Sans
under `assets\fonts\` so Studio's text renders on a machine with no font it
knows. It needs the packages `openepl build --os windows` needs, plus
`tools/build-rmlui-windows.sh` run.

What is different there, said here rather than discovered:

- **Studio runs `openepl.exe` beside itself.** The toolbox, the welcome
  screen's templates and version, `inspect`, the language server and the
  build all go through it. The compiler cross-builds for
  `x86_64-pc-windows-gnu` with two `cfg(windows)` shims still to land in
  `cli/src` (a symlink and the `dlopen` of a library's metadata), and
  `tools/package-windows.sh` ships it when that build succeeds and says
  plainly when it does not. Without it the welcome screen lists no templates
  and the toolbox is empty.
- **Building a program on Windows needs a toolchain on the machine.**
  `openepl.exe` shells out to `clang` for the IR and the C, and to
  mingw-w64's `gcc`/`g++` for the link, so a Windows machine needs
  [LLVM](https://releases.llvm.org/) (clang on `PATH`) and mingw-w64 —
  MSYS2's `mingw-w64-x86_64-gcc` — installed. Without clang, `openepl.exe
  version`, `templates`, `kits`, `project` and `inspect` work, but
  `commands` does not — it compiles each library's metadata with clang —
  so the toolbox is empty, and `build` says `invoke clang: program not
  found`. That is the state as verified under wine; no Windows machine has
  run it.
- **Per-user files** go where Windows keeps them: the recent-projects list
  under `%APPDATA%\openepl\`, the cache under `%LOCALAPPDATA%\openepl\cache\`,
  and a built program under `%TEMP%`. `XDG_DATA_HOME` and `XDG_CACHE_HOME`
  still win when set, which is how a test keeps its scratch out of your list.
- **Accessibility is off**, as it is for a program built for Windows: the
  AccessKit bridge is Unix-only and compiles to stubs under `_WIN32`.
- **Headless runs open a window.** `OPENEPL_DESIGNER_SCRIPT` and
  `OPENEPL_DESIGNER_DUMP` work, but SDL's offscreen driver needs EGL, which
  the Windows build of SDL has none of, so a scripted session there draws
  through an ordinary window.

What has been seen: the image loads under wine with every DLL resolved and
stops where SDL asks a display with its drivers turned off for a window —
the same point a cross-built program reaches — and the child-process layer
Studio's build, run, stop and language-server code sit on passes its own
probe (`designer/test_portable.cpp`) under wine. The drawn Studio window has
not been seen under wine or on Windows: `cargo test --test studio_windows`
checks exactly what is written here, and no further.
