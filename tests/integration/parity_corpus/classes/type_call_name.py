# Pin: `type(x)` returns the class object; `.__name__` is its bare name.
# Expected stdout: `int`.
print(type(1).__name__)
