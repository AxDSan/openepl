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

  let answer: int = 6 * 7
  call print_text(concat("six times seven is ", int_to_text(answer)))
end
```

Then:

```sh
openepl run hello.oir
```

`openepl build hello.oir` leaves a binary called `hello` next to the source
instead of running it. It has no dependency on OpenEPL — copy it to another
machine of the same platform and it runs.

## What the pieces mean

- **`module hello`** names the compilation unit. Every file starts with one.
- **`target console`** says what to build. Leave it out and OpenEPL infers it:
  a module with a form is a windowed program, anything else is a console one.
- **`sub main`** is where a console program starts.
- **`call`** invokes a command for its effect. When you want its result
  instead, use it in an expression: `let n: int = max_int(3, 9)`.
- **`let`** declares a value that will not change. Use `var` when it will.

Continue with [Your first GUI app](./first-gui-app.md), or read the
[Language guide](./language.md).
