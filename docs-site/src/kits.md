# Kits

A kit is a support library together with what an IDE needs to present it: a
display name, a toolbox section, an icon for each component it contributes, a
version, and the project templates it ships.

The split it rests on is already there. `<name>_libinfo.c` names every command
and compiles into a small shared object that the compiler `dlopen`s at build
time and never ships; the implementation sources static-link into your program
and everything unreached is stripped out. Delphi called those the design-time
package and the runtime package, and shipped both — which is why a Delphi
program arrived with a row of DLLs beside it and a version to match. Here the
runtime half ends up inside the executable, so a kit is a build-time thing
only. Your program has no idea a kit was involved.

## Using one

```
module report
use units

sub main
  call print_double(units_c_to_f(21.5))
end
```

`use` names the directory. Nothing is registered, and there is no build file to
edit.

## Where a kit is found

Three places, and the first match of a name wins:

| | Where | For |
| --- | --- | --- |
| 1 | `kits/` beside your project | a kit that belongs to this program |
| 2 | `~/.openepl/kits/` | a kit you installed |
| 3 | the bundled `libs/` | what ships with the toolchain |

So a project can pin its own copy of a kit without installing anything, and an
installed kit can stand in for a bundled library while you work on it.

`openepl kits` prints what resolution decided:

```text
kit: units 1.0.0 project
path: units /home/you/report/kits/units
name: units Units
section: units Measurement
template: units units-app
```

The tier and the path are there because shadowing is the thing that goes wrong.
"It works on my machine" is nearly always a question about which copy of
something was loaded, and it should be answerable without guessing.

## Installing

```sh
openepl kit add ./mykit          # a directory
openepl kit add mykit.tar.gz     # or a tarball
```

Both unpack into `~/.openepl/kits/`. The kit is the directory holding the
`*_libinfo.c` — at the top of the archive or one level down, either works —
and installing over an existing one replaces it and says so.

## Writing one

A kit is a library directory, so start from
[Writing a support library](https://github.com/openepl/openepl/blob/main/libs/README.md)
and add a `lib.json` with the design-time keys:

```json
{
  "display": "Units",
  "section": "Measurement",
  "version": "1.0.0",
  "order": 50,
  "icons": ["Gauge=assets/gauge.svg"],
  "templates": ["units-app"]
}
```

| Key | What it is |
| --- | --- |
| `display` | the name a person reads; defaults to the directory name |
| `section` | the toolbox heading to file the kit under |
| `version` | reported by `openepl kits`; defaults to `0.0.0` |
| `order` | where it sorts in a toolbox; equal values sort by name |
| `icons` | `Component=path` pairs, the path relative to the kit |
| `templates` | subdirectories holding a `template.meta` |

Every key is optional, which is why the libraries that shipped before kits
existed — none of which have any of them — are listed and used unchanged. The
same file also carries the build flags a library needs, so a kit that wants
C++, `pkg-config` or a vendored dependency configures both in one place.

A template named here appears in `openepl templates` and can be created with
`openepl new`, exactly like a built-in one:

```sh
openepl templates
openepl new units-app converter
```
