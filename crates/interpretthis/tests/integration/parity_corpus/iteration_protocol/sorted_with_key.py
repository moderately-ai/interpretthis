# Pins: `sorted(..., key=lambda s: len(s))` sorts by the key function.
x = sorted(['banana', 'apple', 'cherry'], key=lambda s: len(s))
print(x)
