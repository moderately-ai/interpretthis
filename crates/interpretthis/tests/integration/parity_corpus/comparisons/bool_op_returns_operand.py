# Python's `and`/`or` short-circuit AND return the actual operand value,
# not a coerced bool. `0 or 5` is 5; `None or "default"` is "default".
print(0 or 5)
print(0 and 5)
print(None or "default")
print("first" and "second")
print("" or "fallback")
print([] or [1])
print([1, 2] and [3])
print(0 or "" or None or "found")
