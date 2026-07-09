# textwrap.dedent strips the common leading whitespace from each
# non-empty line. Pins the dedent helper.
import textwrap
print(textwrap.dedent("    hello\n    world\n"))
print(textwrap.dedent("  a\n    b\n  c"))
# Lines with only whitespace are preserved as-is.
print(textwrap.dedent("    a\n\n    b"))
# Single line is unchanged.
print(textwrap.dedent("   hello   "))
