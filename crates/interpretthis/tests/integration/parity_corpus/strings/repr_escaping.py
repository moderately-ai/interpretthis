# `repr()` of a string escapes backslashes, the active quote, and control
# characters, and selects the quote character CPython's way (single quotes
# preferred, double only when the string has a single quote but no double).
# Regression: repr wrapped the raw bytes in single quotes with no escaping.
print(repr("a\\b"))
print(repr("can't"))
print(repr('say "hi"'))
print(repr("both ' and \""))
print(repr("tab\there\nnewline"))
print(repr("carriage\rreturn"))
print(repr("plain text"))

# repr is what containers use for their elements.
print(["a\\b", "c'd", 'e"f'])
print({"key\\": "val\tue"})
print(("x\n", "y\t"))

# The bell control character renders as a hex escape.
print(repr("bell\x07here"))
