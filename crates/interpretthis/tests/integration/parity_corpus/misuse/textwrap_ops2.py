import textwrap
print(textwrap.fill("The quick brown fox jumps", width=10))
print(textwrap.wrap("aaa bbb ccc ddd", width=7))
print(textwrap.shorten("Hello world this is long", width=15))
print(textwrap.dedent("    line1\n    line2"))
print(textwrap.indent("a\nb\nc", "> "))
print(repr(textwrap.fill("word " * 5, width=12)))
