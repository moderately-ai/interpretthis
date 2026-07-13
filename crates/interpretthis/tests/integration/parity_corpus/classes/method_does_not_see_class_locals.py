# In CPython, a method defined inside a class body does NOT have
# access to other names defined in the same class body. The class
# body's local namespace becomes the class's __dict__; methods
# look up free names via the MODULE scope, not the class scope.
#
# So `Counter.count` works (it's a class attribute), but `count`
# alone inside `bump` raises NameError.
class Counter:
    count = 10
    def bump(self):
        # Access via the class qualifier — this works.
        return Counter.count + 1

    def naive(self):
        # Bare reference — CPython: NameError.
        try:
            return count + 1
        except NameError as e:
            return f"NameError: {e}"

c = Counter()
print(c.bump())
print(c.naive())
