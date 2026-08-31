# Language guide

A whole program is one module. Files are UTF-8 and use the extension `.oir`;
`#` starts a comment that runs to the end of the line.

```
# A module is a compilation unit.
module hello
target console

sub main
  call print_text("Hello.")
end
```

## Modules

```
module <name>
target <kind>      # optional
use <library>      # optional, repeatable
```

`target` and `use` come first, then the module's contents. Leave `target` out
and OpenEPL infers it — a module with a form is a windowed program, anything
else is a console one. See [Build targets](./build-targets.md).

`use ui` brings in the visual components; without it, `form` and the component
types are not defined.

## Types

| Type | Holds |
| --- | --- |
| `int` | a whole number |
| `int64` | a wider whole number |
| `double` | a number with a fractional part |
| `text` | UTF-8 text |
| `bool` | `true` or `false` |

Types are written after a colon and are not inferred:

```
let count: int = 3
let ratio: double = 1.5
let name: text = "OpenEPL"
let ready: bool = true
```

There is no implicit conversion between them. Convert explicitly:

```
let n: int = 42
call print_text(concat("answer: ", int_to_text(n)))
```

## Values that change, and values that do not

`let` binds a value that stays put. `var` binds one you intend to reassign.

```
let limit: int = 10       # reassigning this is an error
var total: int = 0
total = total + 5
```

The distinction is enforced: assigning to a `let` is a compile error that says
so, and names which fix to apply.

A `var` at module level is visible to every subroutine in the file — which is
how an event handler and the rest of the program share state:

```
module counters
target gui
use ui

var hits: int = 0

form main_window
  title = "Counter"
  width = 320
  height = 160

  button tap
    text = "Tap me"
    left = 40
    top = 40
    width = 160
    height = 40
    on click: on_tap
  end
end

sub on_tap
  hits = hits + 1
  tap.text = concat("tapped ", int_to_text(hits))
end
```

A module variable's initializer may call commands but may not read another
module variable — order-dependent start-up is a source of bugs that is easier
to forbid than to explain.

## Subroutines

```
sub name
  ...
end
```

A subroutine is an *entry point*: either `main`, or a subroutine bound to a
component's event. `main` is where a console program starts; in a windowed
program it runs before the window appears, if the module has one at all.

Subroutines take no parameters, return nothing, and **cannot yet be called
from other code** — `call` invokes the commands the runtime and libraries
provide, not subroutines you have written. Share work between handlers through
module variables for now. This is a real limit, listed with the others under
[Limitations](./limitations.md).

## Expressions

Arithmetic is `+ - * /` with the usual precedence, and parentheses group.

```
let x: int = 2 + 3 * 4        # 14
let y: int = (2 + 3) * 4      # 20
```

Comparisons produce a `bool`: `=` `<>` `<` `<=` `>` `>=`. Combine them with
`and`, `or` and `not`.

```
let in_range: bool = count >= 1 and count <= 10
let missing: bool = not found
```

`=` compares. Assignment is a statement, never an expression, so `if x = 5`
tests whether `x` is five — it cannot assign by accident.

A command that returns a value can be used anywhere a value fits:

```
let longest: int = max_int(length(first), length(second))
```

See [Commands](./reference-commands.md) for the full list.

## Choosing

```
if temperature > 30
  call print_text("hot")
else if temperature > 15
  call print_text("mild")
else
  call print_text("cold")
end
```

The condition must be a `bool`. There is no truthiness — an `int` is not a
condition, and saying so is a compile error rather than a surprise.

## Repeating

```
var i: int = 0
while i < 3
  call print_int(i)
  i = i + 1
end
```

## Components

A component's properties are read and written with a dot:

```
greeting.text = "Ready."
button_ok.width = 200
```

Which properties exist depends on the component; the compiler checks both the
name and the type, so a typo is an error at build time rather than a control
that silently does nothing. See [Forms and events](./forms-and-events.md).
