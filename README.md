# rust
# 0. Tạo image
docker build --secret id=extra-ca,src=.certs/extra-ca.pem -t rust-boilerplate:tag .
# 1. Gọi API dump
curl --location --request POST 'http://localhost:3000/debug/dhat/snapshot'
# 2. Chạy tool phân tích
node tools/dhat-summary.js dhat-heap/dhat-heap-1794820266-000.json 10 --app-only