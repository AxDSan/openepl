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
sub name(parameter: type, ...): type
  ...
  return value
end
```

Both the parameter list and the return type are optional. A subroutine with
neither is an *entry point*: either `main`, or a subroutine bound to a
component's event. `main` is where a console program starts; in a windowed
program it runs before the window appears, if the module has one at all.

```
sub add(a: int, b: int): int
  return a + b
end
```

A parameter is declared the way a variable is — `name: type` — and is
immutable inside the body: assigning to one would make the call site's
argument a lie about what the subroutine is working with. Copy it into a `var`
if you need working state.

A subroutine that declares a return type must `return` a value of that type on
every path; a subroutine that declares none may `return` with no value to leave
early. An `if` without an `else` is not a complete path, and a `while` never
counts as one — it may not run at all.

You call a subroutine exactly as you call a command: as a statement with
`call`, or anywhere in an expression when it returns a value.

```
module greeting

sub shout(who: text): text
  return concat(uppercase(who), "!")
end

sub main
  call print_text(shout("ada"))
end
```

Commands and subroutines share one namespace, so a subroutine may not take a
library command's name — that would silently change what every existing call
in the file means, and the compiler says so instead.

A subroutine may call itself. Recursion needs no forward declaration:

```
module fibonacci

sub fib(n: int): int
  if n < 2
    return n
  end
  return fib(n - 1) + fib(n - 2)
end

sub main
  call print_int(fib(15))
end
```

An entry point and an event handler are the one place the shape is fixed: the
generated code that calls them has no arguments to pass and nowhere to put a
result, so `main` and any subroutine bound to an event must take no parameters
and return nothing.

## Expressions

Arithmetic is `+ - * / %` with the usual precedence, and parentheses group.
`%` is the remainder, and a leading `-` negates.

```
let x: int = 2 + 3 * 4        # 14
let y: int = (2 + 3) * 4      # 20
let r: int = 17 % 5           # 2
let below: int = -40          # negation, on literals and on expressions
```

Both sides of an arithmetic operator must be the same type: there is no
implicit conversion, so `d + 1` where `d` is a `double` is an error. Write
`d + int_to_double(1)`.

Dividing an integer by zero stops the program with a message on stderr rather
than killing it silently.

`+` on two `text` values joins them — the same thing the `concat` command
does, spelled so that building a sentence does not nest.

```
call print_text("Hello, " + who + " — you are " + int_to_text(age) + " today.")
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

`while` repeats for as long as its condition holds.

```
var i: int = 0
while i < 3
  call print_int(i)
  i = i + 1
end
```

`for` counts. The loop variable is an `int` that belongs to the loop and
cannot be assigned to inside it; `step` counts by something other than one,
and a negative step counts down.

```
for n = 1 to 10
  call print_int(n)
end

for n = 10 to 1 step -2
  call print_int(n)
end
```

The start and the limit are read **once**, before the first turn, so a loop
cannot be lengthened by its own body. `step` is a whole-number literal, which
is what lets the compiler know whether the loop counts up or down.

`break` leaves the innermost loop; `continue` goes straight to its next turn.
Both work in a `while` and in a `for`.

```
for n = 101 to 200
  if n % 7 <> 0
    continue
  end
  call print_int(n)     # the first multiple of 7 above 100
  break
end
```

Local variables are visible for the whole subroutine, so two loops in one
subroutine need two different loop-variable names.

## Components

A component's properties are read and written with a dot:

```
greeting.text = "Ready."
button_ok.width = 200
```

Which properties exist depends on the component; the compiler checks both the
name and the type, so a typo is an error at build time rather than a control
that silently does nothing. See [Forms and events](./forms-and-events.md).
