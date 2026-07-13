# Pins: non-data descriptor is shadowed by instance dict.
class NonData:
    def __get__(self, obj, owner=None):
        return "from-desc"

class C:
    x = NonData()

c = C()
print(c.x)
c.x = "from-inst"
print(c.x)
