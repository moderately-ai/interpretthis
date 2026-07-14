# A plain instance (no __eq__) compares equal to itself by identity. Regression:
# the identity fallback compared the addresses of two separately-cloned
# InstanceValue structs (which share an Arc but are distinct structs), so it was
# false for every pair including true aliases — `a == a` was False, and `x in [x]`
# / list.index / list.remove all failed for plain instances.
class C:
    pass


a = C()
print(a == a)
b = a
print(a == b)
print(a != b)

c = C()
print(a == c)
print(a != c)

# The knock-on effects of a broken identity eq:
print(a in [a])
print(a in [c])
xs = [a, c]
print(xs.index(a))
xs.remove(a)
print(len(xs))
