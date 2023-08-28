import requests
import json


def sync_data_loader(url, name, username, password):
    """
    The sync_data_loader function is used to register a HydroLoader instance with a HydroShare account.

    :param url: Specify the url of the hydroshare instance to which you want to connect
    :param name: The name of the data loader instance
    :param username: The account username
    :param password: The user's password
    :return: A tuple of two values; success, and a status message
    """

    request_url = f'{url}/api/data/data-loaders'
    response = requests.get(request_url, auth=(username, password), timeout=60)

    if response.status_code == 401:
        return False, 'Failed to login with given username and password.'

    elif response.status_code == 403:
        return False, 'The given account does not have permission to access this resource.'

    elif response.status_code != 200:
        return False, 'Failed to retrieve account HydroLoader instances.'

    data_loaders = json.loads(response.content)

    if name not in [
        data_loader['name'] for data_loader in data_loaders
    ]:
        response = requests.post(
            request_url,
            auth=(username, password),
            json={'name': name}
        )

        if response.status_code != 201:
            return False, 'Failed to register HydroLoader instance.'

    return True, ''
