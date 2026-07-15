x = 42
print(f"{x=}")
print(f"{x + 1=}")
name = "World"
print(f"{name!r}")
print(f"{name!s}")
print(f"{3.14159:{2+3}.2f}")
print(f"{{literal braces}}")
print(f"{x:>{10}}")
d = {"key": "value"}
dv = d["key"]
print(f"{dv}")
print(f"{x if x > 0 else -x}")
inner = f"{x}"
print(f"nested {inner}")
items = [1, 2, 3]
print(f"{items}")
print(f"{len(items)=}")
print(f"{x:#06x}")
print(f"{-42:=+8}")
print(f"{1000000:,.2f}")
print(f"{[i**2 for i in range(3)]}")
n = 5
print(f"{n:0{n}d}")
print(f"{3.14:g}")
print(f"result: {sum(range(10))}")
