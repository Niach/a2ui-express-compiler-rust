# Product card grid

Request: A product catalogue: a responsive grid of four product cards, each with an image, name, price, a rating, and an 'Add to cart' button; a search field above the grid.

## Sample A

```
Page title="Product Catalogue"
  Stack
    Input label="Search products" placeholder="Search products" name="search" type="search"
    Grid
      Card
        Stack
          Image
          Heading text="Wireless Headphones"
          Text text="$129.00" variant="lead"
          Rating value=4.5
          Button label="Add to cart" icon="shopping-cart"
      Card
        Stack
          Image
          Heading text="Smart Watch"
          Text text="$249.00" variant="lead"
          Rating value=4
          Button label="Add to cart" icon="shopping-cart"
      Card
        Stack
          Image
          Heading text="Travel Backpack"
          Text text="$79.00" variant="lead"
          Rating value=5
          Button label="Add to cart" icon="shopping-cart"
      Card
        Stack
          Image
          Heading text="Bluetooth Speaker"
          Text text="$59.00" variant="lead"
          Rating value=3.5
          Button label="Add to cart" icon="shopping-cart"
```

## Sample B

```
Page title="Products"
  Stack
    Input label="Search products" placeholder="Search products" value={"@path":"/query"} name="search" type="search"
    Grid
[template card: repeated once per item of its list]
  Card
    Stack
      Image
      Heading text={"@path":"name"}
      Text text={"@path":"priceLabel"} variant="lead"
      Rating value={"@path":"rating"}
      Button label="Add to cart" icon="shopping-cart"
[data model] / = {"query":"","products":[{"id":"p1","name":"Trail Running Shoes","priceLabel":"$89.00","rating":4.5,"image":"https://images.example.com/products/trail-shoes.jpg"},{"id":"p2","name":"Insulated Water Bottle","priceLabel":"$24.50","rating":4,"image":"https://images.example.com/products/water-bottle.jpg"},{"id":"p3","name":"Wireless Headphones","priceLabel":"$129.99","rating":5,"image":"https://images.example.com/products/headphones.jpg"},{"id":"p4","name":"Canvas Backpack","priceLabel":"$59.00","rating":3.5,"image":"https://images.example.com/products/backpack.jpg"}]}
```
