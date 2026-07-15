x = 42
name = "world"
pi = 3.14159
print(f"{x}", f"{x:5}", f"{x:05}", f"{x:#x}", f"{x:+}")
print(f"{name!r}", f"{name!s}", f"{name:>10}", f"{name:^10}")
print(f"{pi:.2f}", f"{pi:10.3f}", f"{pi:e}", f"{pi:g}")
print(f"{x = }", f"{name = }")
w = 10
p = 3
print(f"{pi:{w}.{p}f}", f"{x:{w}d}", f"{name:{'*'}<{w}}")
print(f"{'nested ' + name}", f"{[i for i in range(3)]}", f"{ {k: k*2 for k in range(3)} }")
d = {"a": 1}
print(f"{d['a']}", f"{d}")
print(f"{1 + 2 * 3}", f"{'yes' if x > 40 else 'no'}")
print(f"{x:{'>'}{w}}", f"result: {x!r:>8}")
nums = [1, 2, 3]
print(f"{nums[0]}-{nums[-1]}", f"{len(nums)}")
print(f"{{literal braces}}", f"{x}{{}}{name}")
