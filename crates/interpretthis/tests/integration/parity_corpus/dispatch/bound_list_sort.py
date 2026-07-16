# Snapshot (literal) list.sort -> unobservable, returns None
print(getattr([3, 1, 2], "sort")())
# Place list.sort -> mutates the variable
x = [3, 1, 2]
f = x.sort
print(f())
print(x)
# with key= and reverse=
y = ["ccc", "a", "bb"]
s = y.sort
s(key=len)
print(y)
z = [1, 5, 2, 4, 3]
z.sort.__call__(reverse=True) if hasattr(z.sort, "__call__") else z.sort(reverse=True)
print(z)
w = [3, 1, 2]
getattr(w, "sort")()
print(w)
# sort via bound method in a loop
lists = [[3, 1], [2, 0], [9, 5]]
for lst in lists:
    lst.sort()
print(lists)
