# Checkout form

Request: A checkout step: a shipping address form (full name, street, city, postal code, country select), a delivery option choice between Standard (free) and Express ($9.90), an order summary with subtotal, shipping and total, and a 'Place order' button.

## Sample A

```
Page title="Checkout" subtitle="Shipping and delivery"
  Form name="checkout"
    Stack
      Card title="Shipping address"
        Stack
          Input label="Full name" value={"@path":"/shipping/name"} name="fullName" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/shipping/name"}}},"m]
          Input label="Street" value={"@path":"/shipping/street"} name="street" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/shipping/street"}}},]
          FieldGroup
            Input label="City" value={"@path":"/shipping/city"} name="city" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/shipping/city"}}},"m]
            Input label="Postal code" value={"@path":"/shipping/postal"} name="postalCode" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/shipping/postal"}}},]
          Select label="Country" placeholder="Select a country" value={"@path":"/shipping/country"} name="country" options[6] checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/shipping/country"}}}]
      Card title="Delivery option"
        Radio value={"@path":"/delivery"} name="delivery" options[2]
      Card title="Order summary"
        Section
          Row title="Subtotal" value={"@call":"formatCurrency","args":{"value":{"@path":"/subtota
          Row title="Shipping" value={"@call":"cond","args":{"if":{"@call":"eq","args":{"a":{"@pa
          Row title="Total" value={"@call":"formatCurrency","args":{"value":{"@call":"add","ar
      FormActions
[data model] / = {"shipping":{"name":"","street":"","city":"","postal":"","country":""},"delivery":"standard","subtotal":59}
```

## Sample B

```
Page title="Checkout"
  Form name="checkout"
    Stack
      Card title="Shipping address"
        Stack
          Input label="Full name" value={"@path":"/form/fullName"} name="fullName" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/form/fullName"}}},"m]
          Input label="Street" value={"@path":"/form/street"} name="street" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/form/street"}}},"mes]
          FieldGroup
            Input label="City" value={"@path":"/form/city"} name="city" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/form/city"}}},"messa]
            Input label="Postal code" value={"@path":"/form/postal"} name="postal" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/form/postal"}}},"mes]
          Select label="Country" placeholder="Select a country" value={"@path":"/form/country"} name="country" options[5] checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/form/country"}}},"me]
      Card title="Delivery"
        Radio label="Delivery option" value={"@path":"/form/delivery"} name="delivery" options[2]
      Card title="Order summary"
        Stack
          Row title="Subtotal" value={"@call":"formatCurrency","args":{"value":{"@path":"/order/s
          Row title="Shipping" value={"@call":"cond","args":{"if":{"@call":"eq","args":{"a":{"@pa
          Separator
          Row title="Total" value={"@call":"formatCurrency","args":{"value":{"@call":"add","ar
      FormActions
[data model] {"form":{"delivery":"standard"},"order":{"subtotal":48}}
```
