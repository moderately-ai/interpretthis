# A comprehension `if` condition uses the object's truthiness (__bool__/__len__),
# not a hardcoded "instances are always truthy". Regression: the filter used
# Value::is_truthy, so a falsy instance still passed.
class Empty:
    def __len__(self):
        return 0


class Falsy:
    def __bool__(self):
        return False


class Truthy:
    def __bool__(self):
        return True


xs = [Empty(), Falsy(), Truthy(), 1, 0, "", "a"]

print([bool(x) for x in xs])
print([1 for x in xs if x])                 # only the truthy ones
print(len([x for x in xs if x]))
print({i for i, x in enumerate(xs) if x})   # set comprehension filter
print({i: 1 for i, x in enumerate(xs) if x})  # dict comprehension filter
