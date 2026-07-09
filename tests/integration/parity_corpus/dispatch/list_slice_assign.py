# Pin: list slice assignment (`lst[start:] = [...]`) replaces the slice in place.
# Expected stdout: `[1, 9, 9]`.
lst = [1, 2, 3]
lst[1:] = [9, 9]
print(lst)
