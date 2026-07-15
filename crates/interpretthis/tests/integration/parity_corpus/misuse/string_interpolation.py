name = "World"
count = 42
print(f"Hello {name}!")
print(f"Count: {count:05d}")
print(f"{name!r}")
print(f"{count * 2}")
items = [1, 2, 3]
print(f"Items: {items}")
print(f"Sum: {sum(items)}")
d = {"key": "value"}
print(f"{d['key']}")
print(f"{'nested ' + name}")
x = 3.14159
print(f"Pi is approximately {x:.2f}")
print(f"{count:b}")
print(f"{{escaped}}")
print(f"{name.upper()}")
print(f"result: {10 if count > 40 else 20}")
nested = f"{f'{count}'}"
print(nested)
print(f"{count=}, {name=}")
