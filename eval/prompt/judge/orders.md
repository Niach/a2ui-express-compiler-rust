# Order history

Request: An order history page: a status filter (All, Shipped, Delivered, Cancelled) above a list of five past orders, each showing the order number, date, item count, total and a status badge; pressing an order opens it.

## Sample A

```
Page title="Order history"
  Stack gap="md"
    Segmented variant="segmented" value=$/filter items[4: "All" | "Shipped" | "Delivered" | "Cancelled"]
    List divided=true children=[template order, once per item of $/orders]
[template order: repeated once per item of its list]
  Row title=$number subtitle=concat(values=[$date, " · ", $items]) value=$total chevron="right" pressable=true visible=or(values=[eq(a=$/filter, b="all"), eq(a=$/filter, b=$status)]) on=[press→event openOrder {id: $id}]
    [slot trailing]
      Badge text=$statusLabel variant=$badge
[data model] / = {"filter":"all","orders":[{"id":"1042","number":"#1042","date":"Nov 18, 2026","items":"3 items","total":"$128.40","status":"shipped","statusLabel":"Shipped","badge":"secondary"},{"id":"1038","number":"#1038","date":"Nov 11, 2026","items":"1 item","total":"$24.99","status":"delivered","statusLabel":"Delivered","badge":"default"},{"id":"1031","number":"#1031","date":"Oct 29, 2026","items":"5 items","total":"$212.75","status":"delivered","statusLabel":"Delivered","badge":"default"},{"id":"1027","number":"#1027","date":"Oct 15, 2026","items":"2 items","total":"$59.00","status":"cancelled","statusLabel":"Cancelled","badge":"destructive"},{"id":"1019","number":"#1019","date":"Sep 30, 2026","items":"4 items","total":"$87.20","status":"shipped","statusLabel":"Shipped","badge":"secondary"}]}
```

## Sample B

```
Page title="Order history"
  Segmented items[4: "All" | "Shipped" | "Delivered" | "Cancelled"] value=$/filter on=[change→event filter_orders {status: $/filter}]
  List divided=true children=[template orderRow, once per item of $/orders]
[template orderRow: repeated once per item of its list]
  Row title=$number subtitle=$subtitle meta=$total pressable=true chevron="right" on=[press→event open_order {id: $id}]
    [slot trailing]
      Badge text=$status variant=$variant
[data model] {"filter":"all","orders":[{"id":"ord-10482","number":"Order #10482","subtitle":"Nov 14, 2026 · 3 items","total":"$128.40","status":"Shipped","variant":"secondary"},{"id":"ord-10455","number":"Order #10455","subtitle":"Nov 2, 2026 · 1 item","total":"$54.00","status":"Delivered","variant":"default"},{"id":"ord-10391","number":"Order #10391","subtitle":"Oct 19, 2026 · 5 items","total":"$212.75","status":"Delivered","variant":"default"},{"id":"ord-10327","number":"Order #10327","subtitle":"Oct 3, 2026 · 2 items","total":"$39.98","status":"Cancelled","variant":"destructive"},{"id":"ord-10268","number":"Order #10268","subtitle":"Sep 21, 2026 · 4 items","total":"$96.20","status":"Shipped","variant":"secondary"}]}
```
