# Pins: walrus operator in if/comprehension; chained comparisons;
# bool↔int arithmetic. Heavy in agent-emitted code.
data = [1, 2, 3, 4, 5]
if (n := len(data)) > 3:
    print(f"big: {n}")

print([y for x in data if (y := x * x) > 5])

print(1 < 2 < 3)
print(3 < 2 < 1)
print(1 < 2 > 1)

print(True + True + False)
print(sum([True, False, True, True]))

print(",".join("a,b,,c".split(",")))
print(",a,b,".split(","))

print(2 ** 10)
print(2 ** -2)
print(divmod(13, 4))
