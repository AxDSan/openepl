# Shorthands and sugar

The shorthands added in 0.8.0 are conveniences and nothing more. Every one of
them **desugars**: the compiler rewrites it into something the language already
had — a `concat`, an indexed loop, an `x = x + 1` — before the type checker ever
sees it. So each obeys exactly the rules its longhand does and carries no second
meaning. There are three groups: [assignment and operator
shorthands](#assignment-and-operator-shorthands), [string
interpolation](#string-interpolation), and the [iteration
forms](#iteration-ranges-and-for-each) that sit on top of the counting `for`.

## Assignment and operator shorthands

Each shorthand below rewrites to a plain `=`, `concat` or comparison you could
have typed by hand, so it obeys the ordinary type rules and carries no second
meaning.

### Compound assignment

`target OP= value` changes a target in place; it is `target = target OP value`.

| written | means |
| --- | --- |
| `x += e` | `x = x + e` |
| `x -= e` | `x = x - e` |
| `x *= e` | `x = x * e` |
| `x /= e` | `x = x / e` |
| `x mod= e` | `x = x % e` (remainder) |
| `s &= e` | `s = concat(s, e)` (join text) |

The target is any place a plain `=` accepts — a variable, a component property,
an array element, a field inside a c-record:

```
module m
sub main
  var total: int = 0
  total += 10
  total *= 3
  call print_int(total)          # 30

  var greeting: text = "hello"
  greeting &= ", world"
  call print_text(greeting)      # hello, world
end
```

`&=` joins text and only text; `+=` on numbers is arithmetic. The type rules are
the ordinary ones, because the desugar reuses the ordinary `+`, `concat` and `=`.

### increment and decrement

`increment x` is `x = x + 1`; `decrement x` is `x = x - 1`. They are statements,
not expressions — there is no `y = increment x`. Both are soft keywords: a
variable named `increment` still works, because the statement is recognised only
when a target name follows the word.

```
module m
sub main
  var n: int = 0
  increment n
  increment n
  decrement n
  call print_int(n)              # 1
end
```

### Joining and repeating text

`+` between two texts joins them, the same as `concat`. `text * count` repeats a
text, the same as `repeat` — the text goes on the left.

```
module m
sub main
  call print_text("ab" + "cd")   # abcd
  call print_text("=" * 10)      # ==========
end
```

`+` never turns a number into its text: `"n=" + 5` is an error, not `"n=5"`.
Build a mixed message with `concat("n=", int_to_text(5))` instead. Keeping `+`
from ever guessing is deliberate — turning values into text is a job of its own.

### Chained comparison

`1 <= x <= 12` means `1 <= x and x <= 12`, the reading from mathematics. Any two
comparisons that share a middle value chain this way, and the middle is
evaluated **once** — so a call in the middle runs a single time:

```
module m
sub main
  var x: int = 5
  if 0 <= x <= 9
    call print_text("one digit")
  end
end
```

Three comparisons in a row have no single meaning and stay an error; write the
rest with `and`.

### Membership: in and not in

`in` asks whether a value is present; `not in` is its negation. What it lowers to
depends on the right-hand side:

| written | means |
| --- | --- |
| `e in xs` (array) | `index_of(xs, e) <> 0` |
| `k in d` (dictionary) | `dict_has(d, k)` |
| `sub in text` | `find(text, sub) <> 0` |

Positions count from 1, so 0 means "absent". `in` is a soft keyword; a variable
named `in` is untouched.

```
module m
sub main
  var days: text[] = ["mon", "tue", "wed"]
  if "tue" in days
    call print_text("found tuesday")
  end
  if "fun" not in days
    call print_text("no fun today")
  end
  if "ell" in "hello"
    call print_text("a substring")
  end
end
```

### One-line if

A single simple statement — a call, an assignment, a `return`, or an
`increment` — may carry a trailing `if`. `STMT if COND` is exactly
`if COND` / `STMT` / `end`:

```
module m
sub classify(n: int): text
  return "negative" if n < 0
  return "zero" if n = 0
  return "positive"
end
sub main
  call print_text(classify(0-3))
  call print_text(classify(7))
end
```

The block `if` — with a body and an optional `else` — is unchanged.

### Underscores in numbers

A long number may be grouped with underscores, between digits: `1_000_000`,
`0.000_1`. This already worked for hex and binary (`0xDEAD_BEEF`).

### Trailing commas

A single trailing comma is tolerated wherever a comma-separated list is written
— argument and parameter lists, array and dictionary literals, record fields —
so a line can be reordered or extended without minding the last comma:

```
module m
sub main
  var xs: int[] = [1, 2, 3,]
  var ages: int{} = {"ann": 30, "bo": 41,}
  call print_int(count(xs),)       # 3
  call print_int(ages["ann"],)     # 30
end
```

## String interpolation

A text literal may carry **holes** — `{` an expression `}` — and each hole is
replaced, when the program runs, by that expression turned into text:

```
module interp
sub greet(name: text, unread: int): text
  return "Hi {name}, you have {unread} messages."
end
sub main
  call print_text(greet("Ada", 3))   # Hi Ada, you have 3 messages.
end
```

A hole holds a whole expression, not just a name — arithmetic, a field, a call:

```
module holes
record cart
  items: int
  total: double
end
sub main
  let c: cart = cart(items: 3, total: 19.5)
  call print_text("{c.items} items, {c.total} each side of tax")
  call print_text("subtotal ~ {c.items * 10}")
end
```

The grammar of one hole is:

```text
hole   = "{" expression "}"
```

The expression is the ordinary expression language, parsed up to the `}` that
closes the hole; braces **inside** it (a dictionary literal) balance, so the
hole ends at the matching `}`, not the first one. A hole may not itself contain
a text literal — the quote would close the surrounding string first.

Each hole is turned to text by its type, the same conversion a component
property assignment already performs:

| Hole type | Becomes |
|-----------|---------|
| `text` | itself |
| `int` | `int_to_text` |
| `int64` | `int64_to_text` |
| `double` | `double_to_text` |
| `bool` | the word `true` or `false` |

A type with no text form — a `ptr`, an array, a record, a dictionary — is a
build error that names the hole; convert it yourself first.

A brace you mean literally is doubled: `{{` is one `{` and `}}` is one `}`.

```
module braces
sub main
  call print_text("a set is written {{ }} in maths")   # a set is written { } in maths
end
```

Interpolation is **pure sugar**: the literal desugars to the `concat` chain an
author could have written by hand, one call per join, with the right
`*_to_text` around each hole. `"Row {i} of {n}"` is exactly

```text
concat(concat(concat("Row ", int_to_text(i)), " of "), int_to_text(n))
```

so a literal with no holes is unchanged — one string, no `concat` — and a lone
`"{x}"` is just the one conversion. Because `{` now opens a hole in every text
literal, existing braces in a string (JSON, a CSS rule) must be doubled.

A colon inside a hole is **reserved** for a formatting spec that does not exist
yet, so `"{n:04}"` is a clear error rather than a mis-parse — it is not part of
the language today.

## Iteration: ranges and for-each

Two shorthands sit on top of the counting `for`. Both are pure convenience:
each one turns into the counting [`for`](./language.md#repeating), counting from
one, so everything the `for` guarantees — the once-only bounds, the immutable
loop binding, `break` and `continue` — is true of them too.

A **range loop** writes the bounds with `..` instead of `= … to …`. It counts
inclusively from the first bound to the second, and `step` works exactly as it
does on the counting `for` (a negative step counts down).

```
for i in 1..10
  call print_int(i)        # 1, 2, … 10
end

for i in 10..1 step -1
  call print_int(i)        # 10, 9, … 1
end
```

`for i in A..B` is the same loop as `for i = A to B`; the bounds count with
`int`.

**`for each`** walks a collection and binds its elements, so you never spell the
index yourself. It reads an array's items, a byte-set's bytes (each as an
`int`), a text's characters (each as a one-character text), and a dictionary's
keys:

```
for each name in names          # an array of text
  call print_text(name)
end

for each b in payload           # a byte-set; b is an int, 0..255
  call print_int(b)
end
```

Over a dictionary, two bindings read the key and the value together; one binding
reads the key alone:

```
for each key, count in tallies  # an int{}
  call print_text("{key}: {count}")
end

for each key in tallies
  call print_text(key)
end
```

Add `at` to bind the 1-based position alongside the element:

```
for each line at n in lines
  call print_text("{n}: {line}")
end
```

The element, the value and the index bindings are fresh and immutable inside the
loop, just like a `for` counter, and the collection is read once before the
first turn. `break` and `continue` behave as they do in any loop.

`examples/sugar_tour.oir` puts the whole chapter — compound assignment, text
`+`, interpolation, a range loop, `for each` over a dictionary, `in`, and a
one-line `if` — into one short program.
