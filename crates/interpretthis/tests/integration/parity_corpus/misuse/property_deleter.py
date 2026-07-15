class Temperature:
    def __init__(self):
        self._celsius = 0
    @property
    def celsius(self):
        return self._celsius
    @celsius.setter
    def celsius(self, value):
        self._celsius = value
    @celsius.deleter
    def celsius(self):
        print("deleting")
        self._celsius = None
t = Temperature()
t.celsius = 25
print(t.celsius)
del t.celsius
print(t.celsius)
class Circle:
    def __init__(self, radius):
        self.radius = radius
    @property
    def area(self):
        return 3.14159 * self.radius ** 2
    @property
    def diameter(self):
        return self.radius * 2
c = Circle(5)
print(round(c.area, 2))
print(c.diameter)
