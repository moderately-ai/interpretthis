class Circle:
    def __init__(self, radius):
        self._radius = radius
    @property
    def radius(self):
        return self._radius
    @radius.setter
    def radius(self, value):
        if value < 0:
            raise ValueError("negative radius")
        self._radius = value
    @property
    def area(self):
        return 3.14159 * self._radius ** 2
    @property
    def diameter(self):
        return self._radius * 2
    @diameter.setter
    def diameter(self, value):
        self._radius = value / 2
c = Circle(5)
print(c.radius)
print(round(c.area, 2))
print(c.diameter)
c.radius = 10
print(c.radius)
c.diameter = 20
print(c.radius)
try:
    c.radius = -5
except ValueError as e:
    print(str(e))
class Temperature:
    def __init__(self):
        self._celsius = 0
    @property
    def celsius(self):
        return self._celsius
    @celsius.setter
    def celsius(self, value):
        self._celsius = value
    @property
    def fahrenheit(self):
        return self._celsius * 9/5 + 32
t = Temperature()
t.celsius = 25
print(t.fahrenheit)
