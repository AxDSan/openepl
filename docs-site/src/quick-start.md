# Quick start

## From the IDE

```sh
bin/openepl-studio
```

Studio opens on a welcome screen. Pick a project kind and it is created and
opened for you.

![Choosing a project template](./assets/screenshot-welcome.png)

## From the command line

```sh
openepl templates                 # what you can create
openepl new console-app hello     # create a project
openepl run hello/main.oir        # build it and run it
```

Every template builds and runs the moment it is created — `openepl new` is
never a starting point you have to repair first.

That last command prints:

```text
Hello from OpenEPL.
six times seven is 42
```

## Writing it yourself

Create `hello.oir`:

```
module hello
target console

sub main
  call print_text("Hello from OpenEPL.")

  let answer = 6 * 7
  call print_text("six times seven is {answer}")
end
```

Then:

```sh
openepl run hello.oir
```

`openepl build hello.oir` leaves a binary called `hello` next to the source
instead of running it. It has no dependency on OpenEPL — copy it to another
machine of the same platform and it runs.

## A program that waits

`console-app` prints and stops. Some programs should not: they wait for
something and act when it arrives. `examples/loopdemo.oir` is that shape.

```sh
openepl run examples/loopdemo.oir
```

```text
main returned; the timer keeps the program alive
tick 1
tick 2
tick 3
```

`main` printed one line and returned — and the program kept running, because
the module declares a `timer`, and the runtime stays in its event loop while
any source is live. It ends when the tick handler calls `quit()`. A timer draws
nothing, so it is declared at module level rather than inside a form: a program
that waits for something needs no window at all.

## In your editor

`openepl lsp` is a language server, and every editor that speaks LSP can use
it. You get errors underlined as you type, completion for commands, components
and your own subroutines, the parameter list while you are typing a call, and
go-to-definition on any name.

```sh
openepl lsp        # started by your editor, not by you
```

[Editor setup](./editors.md) has
ready-made configuration for Neovim, VS Code, Helix and Zed. The language
server resolves libraries and kits exactly as the compiler does, so anything it
underlines is something that would genuinely fail to build.

## What the pieces mean

- **`module hello`** names the compilation unit. Every file starts with one.
- **`target console`** says what to build. Leave it out and OpenEPL infers it:
  a module with a form is a windowed program, anything else is a console one.
- **`sub main`** is where a console program starts.
- **`call`** invokes a command for its effect. When you want its result
  instead, use it in an expression: `let n: int = max_int(3, 9)`.
- **`let`** declares a value that will not change. Use `var` when it will.
- **Everything counts from 1.** The first element of an array is `a[1]`, the
  first character of a text is at position 1, and `0` is free to mean *not
  found* — which is what `find` answers when there is nothing there.

Continue with [Your first GUI app](./first-gui-app.md), take
[A tour of the language](./tour.md), or read the
[Language guide](./language.md).
