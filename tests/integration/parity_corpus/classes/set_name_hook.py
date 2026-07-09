# Pins: descriptor __set_name__ is called when the class body finishes.
class Named:
    def __set_name__(self, owner, name):
        self.owner = owner.__name__
        self.name = name

class C:
    x = Named()
    y = Named()

print(C.x.name, C.x.owner)
print(C.y.name, C.y.owner)
