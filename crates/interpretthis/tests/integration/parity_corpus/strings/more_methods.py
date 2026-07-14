# str.splitlines / isidentifier / istitle / isprintable / isascii / isnumeric /
# translate / maketrans — previously missing.
print("a\nb\r\nc\rd".splitlines())
print("a\nb".splitlines(True))
print("abc".isidentifier(), "1abc".isidentifier(), "".isidentifier(), "_x".isidentifier())
print("Hello World".istitle(), "hello".istitle(), "HELLO".istitle())
print("hello".isprintable(), "a\tb".isprintable())
print("abc".isascii(), "café".isascii())
print("123".isnumeric(), "12a".isnumeric())
table = str.maketrans("abc", "xyz")
print("cabbage".translate(table))
print("hello".translate(str.maketrans("l", "L")))
print("abcdef".translate(str.maketrans("", "", "aeiou")))
print("abc".translate({97: "X", 98: None, 99: 100}))
