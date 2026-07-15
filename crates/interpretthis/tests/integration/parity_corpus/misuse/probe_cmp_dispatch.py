class Version:
    def __init__(self, major, minor):
        self.major, self.minor = major, minor
    def __eq__(self, o):
        return (self.major, self.minor) == (o.major, o.minor)
    def __lt__(self, o):
        return (self.major, self.minor) < (o.major, o.minor)
    def __le__(self, o):
        return (self.major, self.minor) <= (o.major, o.minor)
    def __gt__(self, o):
        return (self.major, self.minor) > (o.major, o.minor)
    def __ge__(self, o):
        return (self.major, self.minor) >= (o.major, o.minor)
    def __repr__(self):
        return f"v{self.major}.{self.minor}"
v1 = Version(1, 0)
v2 = Version(1, 5)
print(v1 < v2, v1 > v2, v1 <= v2, v1 >= v2)
print(v1 == Version(1, 0), v1 != v2)
versions = [Version(2, 1), Version(1, 0), Version(1, 5)]
print(sorted(versions))
print(min(versions), max(versions))
print(v1 < v2 < Version(2, 0))
print([v for v in versions if v > Version(1, 0)])
