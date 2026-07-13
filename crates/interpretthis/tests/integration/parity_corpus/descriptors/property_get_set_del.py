# @property data descriptor with getter, setter, deleter. Pins the
# eval_attribute property-read intercept, the eval_assign property-
# write intercept (via Expr::Attribute), and the delete property
# branch in eval_delete.
class Account:
    def __init__(self, balance):
        self.storage = balance

    @property
    def balance(self):
        return self.storage

    @balance.setter
    def balance(self, value):
        if value < 0:
            raise ValueError("balance cannot be negative")
        self.storage = value

    @balance.deleter
    def balance(self):
        self.storage = 0

a = Account(100)
print(a.balance)        # getter -> 100
a.balance = 250         # setter
print(a.balance)        # 250
try:
    a.balance = -5
except ValueError as e:
    print("ValueError")
del a.balance           # deleter sets to 0
print(a.balance)        # 0
