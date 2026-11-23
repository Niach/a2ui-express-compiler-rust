# Checkout form

Request: A checkout step: a shipping address form (full name, street, city, postal code, country select), a delivery option choice between Standard (free) and Express ($9.90), an order summary with subtotal, shipping and total, and a 'Place order' button.

## Sample A

```
Page title="Checkout"
  Form name="checkout" on=[submit→event place_order {name: $/form/name, street: $/form/street, city: $/form/city, postal: $/form/postal, country: $/form/country, delivery: $/form/delivery}]
    Stack gap="md"
      Card title="Shipping address"
        Stack gap="md"
          Input label="Full name" name="name" value=$/form/name checks=[required(value=$/form/name) "Required check failed"]
          Input label="Street" name="street" value=$/form/street checks=[required(value=$/form/street) "Required check failed"]
          FieldGroup columns=2
            Input label="City" name="city" value=$/form/city checks=[required(value=$/form/city) "Required check failed"]
            Input label="Postal code" name="postal" value=$/form/postal checks=[required(value=$/form/postal) "Required check failed"]
          Select label="Country" name="country" placeholder="Select a country" value=$/form/country options[5: "Switzerland" | "Germany" | "Austria" | "France" | "United States"] checks=[required(value=$/form/country) "Required check failed"]
      Card title="Delivery"
        Radio label="Delivery option" name="delivery" value=$/form/delivery options[2: "Standard (free)" | "Express ($9.90)"]
      Card title="Order summary"
        List
          Row title="Subtotal" meta=formatCurrency(value=$/order/subtotal, currency="USD", decimals=2)
          Row title="Shipping" meta=cond(if=eq(a=$/form/delivery, b="express"), then=formatCurrency(value=9.9, currency="USD", decimals=2), else="Free")
          Row title="Total" meta=formatCurrency(value=add(a=$/order/subtotal, b=cond(if=eq(a=$/form/delivery, b="express"), then=9.9, else=0)), currency="USD", decimals=2)
      FormActions submit="Place order"
[data model] {"form":{"name":"","street":"","city":"","postal":"","country":"","delivery":"standard"},"order":{"subtotal":48.5}}
```

## Sample B

```
Page title="Checkout" subtitle="Shipping and delivery"
  Form name="checkout" summary=true on=[submit→event placeOrder {fullName: $/fullName, street: $/street, city: $/city, postalCode: $/postalCode, country: $/country, delivery: $/delivery}]
    Stack gap="lg"
      Card title="Shipping address"
        Stack gap="md"
          Input name="fullName" label="Full name" value=$/fullName checks=[required(value=$/fullName) "Enter your full name"]
          Input name="street" label="Street" value=$/street checks=[required(value=$/street) "Enter your street address"]
          FieldGroup columns=2
            Input name="city" label="City" value=$/city checks=[required(value=$/city) "Enter your city"]
            Input name="postalCode" label="Postal code" value=$/postalCode checks=[required(value=$/postalCode) "Enter your postal code"]
          Select name="country" label="Country" placeholder="Select a country" value=$/country options[7: "Austria" | "France" | "Germany" | "Italy" | "Switzerland" | "United Kingdom" | "United States"] checks=[required(value=$/country) "Select a country"]
      Card title="Delivery option"
        Radio name="delivery" value=$/delivery options[2: "Standard" | "Express"]
      Card title="Order summary"
        Stack gap="sm"
          Row title="Subtotal" value=formatCurrency(value=$/subtotal, currency="USD", decimals=2)
          Row title="Shipping" value=cond(if=eq(a=$/delivery, b="express"), then="$9.90", else="Free")
          Separator
          Row title="Total" value=formatCurrency(value=add(a=$/subtotal, b=cond(if=eq(a=$/delivery, b="express"), then=9.9, else=0)), currency="USD", decimals=2)
      FormActions submit="Place order"
[data model] / = {"delivery":"standard","subtotal":89,"fullName":"","street":"","city":"","postalCode":"","country":""}
```
