# iter(list) shares the list (CPython's list_iterator), so mutations before the
# cursor reaches them are observed — not an eager snapshot.
lst = [1, 2, 3]
it = iter(lst)
print(next(it))
lst.append(4)
print(list(it))

lst2 = [1, 2, 3, 4, 5]
it2 = iter(lst2)
print(next(it2), next(it2))
del lst2[2:]
print(list(it2))

print(type(iter([1, 2])).__name__)
print(list(iter([1, 2, 3])))
print(list(iter([])))
print([x for x in iter([10, 20, 30])])

xs = [1]
it3 = iter(xs)
print(next(it3))
xs.append(2)
xs.append(3)
print(next(it3), next(it3))
try:
    next(it3)
except StopIteration:
    print("stopped")

print(sum(iter([1, 2, 3, 4])))

a = [5, 6, 7]
b = iter(a)
a[1] = 60
print(list(b))


# A user __iter__ returning iter(list) still iterates correctly.
class Fixed:
    def __iter__(self):
        return iter(["a", "b", "c"])


print([*Fixed()])
print(list(Fixed()))
