# Forms and events

A windowed program declares a `form`. The form is the window; the components
inside it are what the window contains.

```
module my_app
target gui
use ui

form main_window
  title = "My App"
  width = 480
  height = 300
  background_color = "#1e2233"

  label greeting
    text = "Click the button."
    left = 40
    top = 50
    width = 400
    color = "#ffffff"
  end

  button ok_button
    text = "Click me"
    left = 40
    top = 110
    width = 160
    height = 44
    background_color = "#4a86e8"
    border_radius = 8
    on click: on_ok_click
  end
end

sub on_ok_click
  greeting.text = "Button clicked."
end
```

## The shape

```
form <name>
  <property> = <value>       # the window's own properties

  <type> <name>              # a component
    <property> = <value>
    on <event>: <subroutine>
  end
end
```

A module has one form. Component names are unique within the file and share
one namespace with subroutines and module variables, so nothing can be two
things at once.

A component with no rectangle — a `timer`, an `action`, a `datasource`, an
`httpserver` — is not declared inside the form but beside it, at module
level, in the same block shape. A console program can declare one without any
form at all; that is how a program waits for something. See
[Components](./components.md).

## Properties

Properties are set with literals here, and with ordinary statements once the
program is running:

```
greeting.text = "Ready."
```

Both are checked at build time — an unknown property, or a value of the wrong
type, is a compile error naming the component and the property. Colours are
`text` holding a hex value such as `"#4a86e8"`.

Every component and every property is listed in
[Components](./reference-components.md).

## Events

`on <event>: <subroutine>` binds an event to a subroutine you have written.

```
  button ok_button
    on click: on_ok_click
  end
```

Bindings are resolved when the program is built: if the subroutine does not
exist, the build fails rather than the button quietly doing nothing.

Some events hand the handler a value — a `grid`'s `select` hands the row, a
`timer`'s `tick` hands the tick count. The handler takes exactly that, or
nothing at all; the compiler makes the two agree and shows the header to paste
when they do not. `click` hands nothing, so a click handler is always the
plain shape above. What each event hands over is in
[Components](./components.md).

## Doing this visually

You do not have to write any of this by hand — the designer produces exactly
this shape, and reads it back. Drag a button onto the canvas and the `button`
block appears; type in the inspector and the property changes; wire an event
and the binding is written, along with an empty subroutine if you have not
made one yet.

There is no separate designer file. What you draw and what you edit are the
same source, which is why they cannot disagree.

## Order of events

For a windowed program:

1. the window and its components are created,
2. `main` runs, if the module has one,
3. the event loop starts and your handlers run as things happen.

`main` is optional here. It is a place for setup that has to happen before
anything is shown.

## Accessibility

Components carry an accessibility role and name, and the running program
publishes a live accessibility tree that assistive technology can read. This
is part of the component model rather than something bolted on afterwards, so
it is true of anything you build without extra work.
