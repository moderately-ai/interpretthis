# Pins: class bases from attribute expressions (module.Class).
class Base:
    def tag(self):
        return "base"

class Holder:
    Parent = Base

class Child(Holder.Parent):
    pass

print(Child().tag())
