select month, orders
from monthly_orders
order by strptime(month, '%b');
