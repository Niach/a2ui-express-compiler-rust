# Product card grid

Request: A product catalogue: a responsive grid of four product cards, each with an image, name, price, a rating, and an 'Add to cart' button; a search field above the grid.

## Sample A

```
Page title="Products"
  Stack gap="lg"
    Input label="Search products" name="query" type="search" placeholder="Search the catalogue" value=$/query on=[change→event search {query: $/query}]
    Grid columns={base: 1, sm: 2, lg: 4} gap="md" children=[template productCard, once per item of $/products]
[template productCard: repeated once per item of its list]
  Card
    Image src=$image alt=$name fit="cover" aspectRatio=1.33
    Heading text=$name level="h3"
    Text text=$price variant="lead"
    Rating value=$rating max=5 readOnly=true
    Button label="Add to cart" icon="shopping-cart" on=[press→event add_to_cart {id: $id}]
[data model] {"query":"","products":[{"id":"p1","name":"Trail Runner Shoes","price":"$89.00","rating":4.5,"image":"https://picsum.photos/seed/shoes/400/300"},{"id":"p2","name":"Insulated Water Bottle","price":"$24.50","rating":4,"image":"https://picsum.photos/seed/bottle/400/300"},{"id":"p3","name":"Wireless Headphones","price":"$129.99","rating":5,"image":"https://picsum.photos/seed/headphones/400/300"},{"id":"p4","name":"Canvas Backpack","price":"$64.00","rating":3.5,"image":"https://picsum.photos/seed/backpack/400/300"}]}
```

## Sample B

```
Page title="Products"
  Stack gap="lg"
    Input name="search" label="Search products" type="search" placeholder="Search products" value=$/query
    Grid columns={base: 1, sm: 2, lg: 4} gap="md" children=[template card, once per item of $/products]
[template card: repeated once per item of its list]
  Card padded=true
    Stack gap="sm"
      Image src=$image alt=$name aspectRatio=1 fit="cover" fallback="image"
      Heading level="h3" text=$name
      Text variant="lead" text=$price
      Rating value=$rating max=5 readOnly=true size="sm"
      Button label="Add to cart" icon="shopping-cart" on=[press→event addToCart {productId: $id}]
[data model] /query = ""; /products = [{"id":"p1","name":"Canvas Backpack","price":"$59.00","rating":4.5,"image":"https://picsum.photos/seed/backpack/400/400"},{"id":"p2","name":"Ceramic Mug","price":"$18.00","rating":4,"image":"https://picsum.photos/seed/mug/400/400"},{"id":"p3","name":"Wireless Headphones","price":"$129.00","rating":5,"image":"https://picsum.photos/seed/headphones/400/400"},{"id":"p4","name":"Desk Lamp","price":"$42.00","rating":3.5,"image":"https://picsum.photos/seed/lamp/400/400"}]
```
