# textwrap.wrap splits at word boundaries respecting the width.
import textwrap
print(textwrap.wrap("hello world this is a test", 10))
print(textwrap.wrap("short", 10))
print(textwrap.wrap("", 10))
print(textwrap.fill("hello world this is a test", 10))
