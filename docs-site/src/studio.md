# The IDE

`openepl-studio` with no arguments opens the welcome screen; with a `.oir`
file it opens that project.

![The OpenEPL Studio visual designer](./assets/screenshot-designer.png)

## The layout

- **Toolbox** — the components you can place. Search narrows it. Greyed items
  are declared but not implemented yet.
- **Canvas** — the form as it will look. Drag to move, drag the handles to
  resize, drag the form's corner to resize the window.
- **Inspector** — every property of the selection, and its events on the
  second tab. Type a value and press Enter.
- **Code preview** — the source for whatever is selected. Click it to open the
  editor.
- **Problems** — what the language server has found, updated as you type.
- **Output** — build progress and whatever your program prints.

The panels are resizable: drag the dividers between them.

## Designer and Code

The two tabs are two views of one file. Anything you do in the designer is
written to the source, and anything you write is read back by the designer.

![The code editor](./assets/screenshot-editor.png)

The editor has syntax highlighting and live diagnostics from the same language
server other editors use. **Ctrl+S** saves. Saving from the editor re-reads
the file, so the canvas follows what you wrote.

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
  stage 4/4  dead-strip unused commands
OK  /tmp/openepl_studio_app — 24101 KB in 5.16s
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
