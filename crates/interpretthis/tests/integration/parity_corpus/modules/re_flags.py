# re compilation flags, positional and keyword, and combined.
import re

print(re.match(r"a", "A", re.IGNORECASE).group())
print(re.findall(r"[a-z]+", "Hello World", re.I))
print(re.sub(r"o", "0", "FOO", flags=re.IGNORECASE))
print(bool(re.search(r"^world", "hello\nworld", re.MULTILINE)))
print(re.findall(r".", "a\nb", re.DOTALL))
print(re.search(r"a.b", "a\nb", re.S).group())
print(re.split(r"x", "aXbxc", flags=re.I))
print(bool(re.match(r"ABC", "abc", re.IGNORECASE)))
print(bool(re.match(r"ABC", "abc")))

p = re.compile(r"hello", re.I)
print(p.match("HELLO").group())
print(p.findall("hello HELLO Hello"))

# Flags combine as integers.
print(re.I | re.M == 10)
print(bool(re.search(r"^b", "a\nB", re.M | re.I)))
