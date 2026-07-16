x = 42
print(f"{x=}")
print(f"{x = }")
print(f"{x=:04d}")
print(f"{x=:>8}")
val = 3.14159
print(f"{val=:.2f}")
print(f"{val = :.3f}")
w = 10
p = 2
print(f"{val:{w}.{p}f}")
print(f"{'text':{'>'}{w}}")
nested = {"key": "value"}
print(f"{nested['key']}")
print(f"{nested['key']!r:>10}")
items = [1, 2, 3]
print(f"{items[1]}")
print(f"{sum(items)=}")
print(f"{len('hello')=}")
print(f"result: {2**10}")
print(f"{x if x > 0 else -x}")
print(f"{', '.join(str(i) for i in range(3))}")
c = complex(1, 2)
print(f"{c=}")
print(f"{c.real=}, {c.imag=}")
d = 255
print(f"{d:#x} {d:#o} {d:#b}")
print(f"{d=:#x}")
print(f"{'a' * 3}")
print(f"{{{x}}}")
print(f"{x:+} {-x:+}")
name = "world"
print(f"{f'{name}'}")
