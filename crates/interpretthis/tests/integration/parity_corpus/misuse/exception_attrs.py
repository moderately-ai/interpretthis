try:
    raise ValueError("msg", 42, "extra")
except ValueError as e:
    print(e.args)
    print(len(e.args))
    print(e.args[1])
try:
    d = {}
    d["missing"]
except KeyError as e:
    print(e.args)
    print(repr(e))
try:
    [1, 2, 3][10]
except IndexError as e:
    print(str(e))
class CustomError(Exception):
    def __init__(self, code, msg):
        self.code = code
        super().__init__(msg)
try:
    raise CustomError(404, "not found")
except CustomError as e:
    print(e.code, str(e))
    print(e.args)
try:
    raise RuntimeError()
except RuntimeError as e:
    print(repr(e.args), str(e))
