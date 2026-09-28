# Markdown Edge Cases

## Fenced code

The line beginning with `#` below is a shell comment, not a Markdown heading.

```bash
# ~/.zshrc or ~/.bashrc
export EXAMPLE_ROOT="$HOME/example"
```

## Heading after code

A parser must preserve this section under `Markdown Edge Cases`, even after the fenced shell block.

## Heading with closing hashes ###

Closing hash characters are optional Markdown decoration and are not part of the heading title.
