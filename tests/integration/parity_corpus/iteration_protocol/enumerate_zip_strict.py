# Pins: enumerate(start=N); zip() length-truncation; zip(strict=True)
# raises ValueError when lengths differ; max/min with default kwarg.
print(max([], default=-1))

for i, v in enumerate(['a', 'b', 'c'], start=10):
    print(i, v)

print(list(zip([1, 2, 3], ['a', 'b'])))

try:
    list(zip([1, 2, 3], [4, 5], strict=True))
    print("oops")
except ValueError as e:
    print("strict raises:", str(e))
