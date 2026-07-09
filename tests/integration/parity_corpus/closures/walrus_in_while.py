# Walrus in a while-loop condition: `while (n := pop()) > 0: ...`.
# The walrus target is local to the function — after returning, n must
# not exist at module scope.
def consume(items):
    total = 0
    i = 0
    while (n := items[i] if i < len(items) else None) is not None:
        total = total + n
        i = i + 1
    return total

print(consume([1, 2, 3]))
try:
    print(n)
except NameError as e:
    print(f"NameError: {e}")
