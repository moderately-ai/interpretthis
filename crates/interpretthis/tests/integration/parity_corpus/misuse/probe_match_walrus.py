def classify(x):
    match x:
        case 0:
            return "zero"
        case int() if x < 0:
            return "negative"
        case [a, b]:
            return f"pair {a},{b}"
        case {"key": v}:
            return f"dict {v}"
        case _:
            return "other"
print(classify(0))
print(classify(-5))
print(classify([1, 2]))
print(classify({"key": 42}))
print(classify("str"))
data = [1, 2, 3, 4, 5]
if (n := len(data)) > 3:
    print(f"long: {n}")
print([y := x*2 for x in range(3)], y)
while (chunk := data[:2]):
    print(chunk)
    data = data[2:]
