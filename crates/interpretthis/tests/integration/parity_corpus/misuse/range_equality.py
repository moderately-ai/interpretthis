# range equality is by yielded sequence, not by field values: two ranges are
# equal iff they produce the same elements. An empty range equals any other
# empty range; a length-1 range ignores step.
print(range(3) == range(3))
print(range(0, 3) == range(0, 3, 1))
print(range(0, 3) == range(0, 3, 2))
print(range(0, 3, 2) == range(0, 4, 2))
print(range(0) == range(5, 5))
print(range(5, 5) == range(10, 3))
print(range(1, 10, 3) == range(1, 9, 3))
print(range(0, 1) == range(0, 1, 99))
print(range(0, 10, 2) == range(0, 9, 2))
print(range(3) == range(4))
print(range(3) == [0, 1, 2])

# Consequences for membership and de-dup once ranges compare by value.
print(range(3) in [range(3), range(4)])
print(range(0, 3, 1) in {range(3): "a"} if False else range(3) == range(0, 3, 1))
