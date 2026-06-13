# RUSH

**RUSH (Rapid Universal SHell)** is a modern cross-platform command shell designed for developers who work on both Windows and Linux.

RUSH aims to bring the simplicity, composability, and productivity of traditional Unix shells to Windows while remaining fully native and first-class on Linux.

## Why RUSH?

Today's shell landscape forces developers to make compromises:

- **PowerShell** is powerful but verbose and significantly different from the Unix ecosystem.
- **Git Bash** provides familiar tools but relies on compatibility layers and often feels disconnected from the operating system.
- **WSL** offers a real Linux environment but introduces an additional layer between the user and the host system.
- **Modern shells** often introduce entirely new paradigms that reduce compatibility with existing tools and scripts.

RUSH takes a different approach:

> Familiar Unix workflows. Native Windows experience. One shell everywhere.

## Goals

### Cross-platform by design

RUSH runs natively on:

- Windows
- Linux

The same commands, scripts, aliases, and workflows should work consistently across platforms whenever possible.

### Text-based pipelines

RUSH embraces the Unix philosophy:

```sh
find . | grep todo | sort | uniq
```

Pipelines are text streams by default, making existing command-line knowledge immediately useful.

### First-class GNU tooling

RUSH aims to provide a familiar Unix environment out of the box.

Examples include:

```text
ls
cp
mv
rm
cat
grep
find
sort
uniq
wc
tar
zip
unzip
awk
sed
```

Developers should be able to follow Linux tutorials and expect commands to behave as expected.

### Modern developer experience

While compatibility matters, RUSH is not a clone of Bash.

Planned improvements include:

- Fast startup
- Rich tab completion
- Better error messages
- Improved scripting syntax
- Unicode support
- Consistent behavior across operating systems
- Discoverable command help
- Extensible plugin system

### Native Windows integration

RUSH treats Windows as a first-class platform.

Examples:

- Native process execution
- Native filesystem support
- Windows environment variable integration
- PowerShell and CMD interoperability when needed
- Support for both Windows and Unix-style paths

```sh
cd C:\Projects
```

```sh
cd ~/projects
```

Both should feel natural.

## Design Principles

### Familiar

A developer who knows Bash should be productive in RUSH within minutes.

### Practical

Compatibility and usability are prioritized over introducing new paradigms.

### Fast

The shell should feel lightweight and responsive, even for everyday interactive use.

### Portable

Scripts should work consistently across supported platforms.

### Extensible

Users should be able to extend RUSH through plugins, custom commands, and automation.

## Example

```sh
# Find all JSON files larger than 1 MB
find . -name "*.json" | filter-size 1mb

# Count matching lines
grep ERROR app.log | wc -l

# Create an archive
tar -czf backup.tar.gz src/
```

## Status

RUSH is currently in early development.

The first implementation currently supports:

- Interactive input with line editing and persistent history
- `rush -c "command"` and UTF-8 script files
- External commands, concurrent pipelines, and exit statuses
- Single and double quotes, escapes, environment expansion, globbing, and `$(...)`
- `&&`, `||`, `;`, and background execution with `&`
- `<`, `>`, `>>`, `2>`, `2>>`, and `2>&1`
- Stateful `cd`, `exit`, `jobs`, and `fg` built-ins
- Process-group-based Ctrl-C forwarding for foreground pipelines and `fg`
- A bundled uutils-based command set through the `rush-utils` companion binary

The bundled command set is currently:

```text
cat cp echo ls mkdir mv pwd rm sort touch uniq wc
```

Build both binaries before running from the repository:

```sh
cargo build --bins
target/debug/rush
```

Background jobs use a portable subset of traditional shell job control. `&`,
`jobs`, and `fg` are available, but stopped jobs, `bg`, Ctrl-Z suspension, and
interactive terminal handoff are not implemented yet.

## Vision

RUSH aims to become the shell developers install on a fresh machine regardless of operating system.

A shell that feels familiar to Unix users, works naturally on Windows, and provides a modern foundation for the next generation of command-line workflows.
