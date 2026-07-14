# A match-case guard uses the guard value's truthiness (__bool__/__len__), not a
# hardcoded "instances are truthy". Regression: the guard used Value::is_truthy.
class Falsy:
    def __bool__(self):
        return False


class Empty:
    def __len__(self):
        return 0


def check(v, g):
    match v:
        case _ if g:
            return "matched"
        case _:
            return "fell through"


print(check(1, Falsy()))
print(check(1, Empty()))
print(check(1, 5))
print(check(1, 0))
