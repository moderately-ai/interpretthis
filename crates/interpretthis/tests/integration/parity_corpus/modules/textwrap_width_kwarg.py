# textwrap.wrap/fill/shorten accept width as a keyword argument. Regression: the
# module call path dropped kwargs, so width= was ignored and the default 70 was
# always used.
import textwrap

print(textwrap.wrap("a b c d e f", width=5))
print(textwrap.wrap("a b c d e f", 5))          # positional still works
print(textwrap.fill("a b c d e f", width=5).replace(chr(10), "|"))
print(textwrap.shorten("a b c d e f g", width=10))
