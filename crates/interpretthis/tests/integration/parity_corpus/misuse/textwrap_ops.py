import textwrap
print(textwrap.fill("a b c d e f g", width=5))
print(textwrap.wrap("hello world foo", width=8))
print(textwrap.shorten("Hello world foo bar", width=12))
print(textwrap.indent("a\nb", "> "))
print(textwrap.dedent("    x\n    y"))
