x = 5
print("positive" if x > 0 else "non-positive")
print([i if i % 2 == 0 else -i for i in range(5)])
result = (lambda n: "even" if n % 2 == 0 else "odd")(7)
print(result)
values = [1, -2, 3, -4]
print([abs(v) if v < 0 else v for v in values])
a, b = 3, 7
print(a if a > b else b)
data = {"key": None}
print(data.get("key") or "default")
print(data.get("missing") or "fallback")
nums = [0, 1, 2, 0, 3]
print([n or -1 for n in nums])
print(True and "yes" or "no")
print(False and "yes" or "no")
print(None or 0 or "" or "found")
print(1 and 2 and 3)
print(0 or None or False)
grade = 85
print("A" if grade >= 90 else "B" if grade >= 80 else "C")
print(max(1, 2) if True else min(1, 2))
